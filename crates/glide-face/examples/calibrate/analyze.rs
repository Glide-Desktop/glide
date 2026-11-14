use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::Path;

use glide_face::biometrics::calibration::verifier_score;
use glide_face::biometrics::{RECOGNIZER_ID, average_embeddings, cosine_similarity};

use super::Sample;

const COHORT_SIZE: usize = 1000;
const ENROLL_IMAGES: usize = 5;
const MAX_GENUINE_PROBES: usize = 15;
const BINS: usize = 4000;

type Rule<'a> = Box<dyn Fn(&Enrollee, f64) -> f64 + 'a>;

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3))
}

fn linfit(pts: &[(f64, f64)]) -> (f64, f64) {
    let n = pts.len() as f64;
    let (sx, sy) = pts.iter().fold((0.0, 0.0), |(a, b), p| (a + p.0, b + p.1));
    let (mx, my) = (sx / n, sy / n);
    let sxx: f64 = pts.iter().map(|p| (p.0 - mx).powi(2)).sum();
    let sxy: f64 = pts.iter().map(|p| (p.0 - mx) * (p.1 - my)).sum();
    let slope = sxy / sxx;
    (my - slope * mx, slope)
}

fn literal(value: f64) -> String {
    let text = format!("{value:.6}");
    let Some((whole, fraction)) = text.split_once('.') else { return text };
    let digits: Vec<String> = fraction.as_bytes().chunks(3).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
    format!("{whole}.{}", digits.join("_"))
}

struct Tail {
    hist: Vec<u64>,
    total: u64,
}

impl Tail {
    fn new() -> Self {
        Self { hist: vec![0; BINS], total: 0 }
    }

    fn add(&mut self, s: f64) {
        let b = (f64::midpoint(s, 1.0) * BINS as f64) as usize;
        self.hist[b.min(BINS - 1)] += 1;
        self.total += 1;
    }

    fn threshold_of_bin(b: usize) -> f64 {
        b as f64 / BINS as f64 * 2.0 - 1.0
    }

    fn threshold_at(&self, target: f64) -> f64 {
        let mut acc = 0u64;
        for b in (0..BINS).rev() {
            acc += self.hist[b];
            if acc as f64 / self.total as f64 > target {
                return Self::threshold_of_bin(b + 1);
            }
        }
        -1.0
    }

    fn fit(&self, min_events: f64, floor: f64) -> (f64, f64, f64) {
        let lowest = (min_events / self.total as f64).max(floor);
        let mut pts = Vec::new();
        let mut acc = 0u64;
        for b in (0..BINS).rev() {
            acc += self.hist[b];
            let far = acc as f64 / self.total as f64;
            if far > 1e-3 {
                break;
            }
            if far >= lowest {
                pts.push((Self::threshold_of_bin(b), far.log10()));
            }
        }
        let (i, s) = linfit(&pts);
        (i, s, lowest)
    }
}

struct Enrollee {
    genuine: Vec<f64>,
    impostor: Vec<f32>,
    cohort_mean: f64,
    cohort_std: f64,
    cohort_q99: f64,
}

fn mean_std(v: &[f64]) -> (f64, f64) {
    let m = v.iter().sum::<f64>() / v.len() as f64;
    (m, (v.iter().map(|x| (x - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt())
}

fn pooled(users: &[Enrollee], rule: &dyn Fn(&Enrollee) -> f64) -> (f64, f64, Vec<f64>) {
    let (mut fa, mut ni, mut fr, mut ng) = (0u64, 0u64, 0u64, 0u64);
    let mut per_user = Vec::with_capacity(users.len());
    for u in users {
        let t = rule(u);
        let a = u.impostor.iter().filter(|&&s| f64::from(s) >= t).count() as u64;
        fa += a;
        ni += u.impostor.len() as u64;
        fr += u.genuine.iter().filter(|&&s| s < t).count() as u64;
        ng += u.genuine.len() as u64;
        per_user.push(a as f64 / u.impostor.len() as f64);
    }
    (fa as f64 / ni as f64, fr as f64 / ng as f64, per_user)
}

fn solve(
    users: &[Enrollee], target: f64, lo: f64, hi: f64, rule: &dyn Fn(&Enrollee, f64) -> f64,
) -> (f64, f64, f64, Vec<f64>) {
    let (mut lo, mut hi) = (lo, hi);
    for _ in 0..40 {
        let mid = f64::midpoint(lo, hi);
        let (far, _, _) = pooled(users, &|u| rule(u, mid));
        if far > target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let (far, frr, per) = pooled(users, &|u| rule(u, hi));
    (hi, far, frr, per)
}

#[allow(clippy::too_many_lines)] // A report script: each stage prints its part of the calibration in order
pub fn run(samples: &[Sample], out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let mut by_id: BTreeMap<&str, Vec<&Sample>> = BTreeMap::new();
    for s in samples {
        by_id.entry(&s.identity).or_default().push(s);
    }
    println!("{} images, {} identities", samples.len(), by_id.len());

    let mut pair_tail = Tail::new();
    let mut genuine_pairs: Vec<f64> = Vec::new();
    for i in 0..samples.len() {
        for j in i + 1..samples.len() {
            let s = cosine_similarity(&samples[i].emb, &samples[j].emb);
            if samples[i].identity == samples[j].identity {
                genuine_pairs.push(s);
            } else {
                pair_tail.add(s);
            }
        }
    }
    genuine_pairs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (pi, ps, plow) = pair_tail.fit(300.0, 1e-5);
    let pair_thr = |far: f64| (far.log10() - pi) / ps;
    println!("\n== single-image pairs: {} impostor, {} genuine", pair_tail.total, genuine_pairs.len());
    println!("tail fit: log10(FAR) = {pi:.3} + {ps:.3} * t   (fitted on FAR 1e-3 .. {plow:.1e})");
    for far in [1e-3, 1e-4, 1e-5] {
        println!("  FAR {far:.0e}: measured threshold {:.3}, fitted {:.3}", pair_tail.threshold_at(far), pair_thr(far));
    }
    let floor = pair_thr(1e-5);

    let mut singles: Vec<&Sample> = by_id
        .values()
        .filter(|v| v.len() == 1)
        .map(|v| v[0])
        .filter(|s| s.yaw.abs() <= 20.0 && s.face_width >= 80.0)
        .collect();
    singles.sort_by_key(|s| fnv(&s.identity));
    let cohort: Vec<&Sample> = singles.iter().take(COHORT_SIZE).copied().collect();
    let cohort_ids: BTreeSet<&str> = cohort.iter().map(|s| s.identity.as_str()).collect();
    let enrollee_ids: Vec<&str> = by_id.iter().filter(|(_, v)| v.len() > ENROLL_IMAGES).map(|(k, _)| *k).collect();
    let impostor_pool: Vec<&Sample> = samples
        .iter()
        .filter(|s| !cohort_ids.contains(s.identity.as_str()) && by_id[s.identity.as_str()].len() <= ENROLL_IMAGES)
        .collect();
    println!(
        "\ncohort {} identities | enrollees {} | impostor-only pool {} images",
        cohort.len(),
        enrollee_ids.len(),
        impostor_pool.len()
    );

    let probes_of =
        |id: &str| -> Vec<&Sample> { by_id[id].iter().skip(ENROLL_IMAGES).take(MAX_GENUINE_PROBES).copied().collect() };
    let mut users: Vec<Enrollee> = Vec::new();
    let mut score_tail = Tail::new();
    for id in &enrollee_ids {
        let multi: Vec<Vec<f32>> = by_id[id].iter().take(ENROLL_IMAGES).map(|s| s.emb.clone()).collect();
        let centroid = average_embeddings(&multi);
        let genuine = probes_of(id).iter().map(|p| verifier_score(&p.emb, &centroid, &multi, floor)).collect();
        let mut impostor: Vec<f32> =
            impostor_pool.iter().map(|p| verifier_score(&p.emb, &centroid, &multi, floor) as f32).collect();
        for other in enrollee_ids.iter().filter(|o| *o != id) {
            impostor.extend(probes_of(other).iter().map(|p| verifier_score(&p.emb, &centroid, &multi, floor) as f32));
        }
        for &s in &impostor {
            score_tail.add(f64::from(s));
        }
        let cs: Vec<f64> = cohort.iter().map(|c| verifier_score(&c.emb, &centroid, &multi, floor)).collect();
        let (cohort_mean, cohort_std) = mean_std(&cs);
        let mut sorted = cs.clone();
        sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
        let cohort_q99 = sorted[sorted.len() / 100];
        users.push(Enrollee { genuine, impostor, cohort_mean, cohort_std, cohort_q99 });
    }
    let n_gen: usize = users.iter().map(|u| u.genuine.len()).sum();
    let (vi, vs, vlow) = score_tail.fit(30.0, 1e-5);
    let score_thr = |far: f64| (far.log10() - vi) / vs;
    println!(
        "\n== verifier score (probe vs 5-template profile): {} impostor trials, {} genuine trials",
        score_tail.total, n_gen
    );
    println!("tail fit: log10(FAR) = {vi:.3} + {vs:.3} * t   (fitted on FAR 1e-3 .. {vlow:.1e})");
    let frr_at = |t: f64| users.iter().flat_map(|u| &u.genuine).filter(|&&s| s < t).count() as f64 / n_gen as f64;
    println!("  FAR      measured-thr  fitted-thr  FRR@fitted");
    for far in [1e-3, 1e-4, 1e-5, 1e-6, 1e-7] {
        let measured = if far >= vlow { format!("{:.3}", score_tail.threshold_at(far)) } else { "  -  ".into() };
        println!(
            "  {far:.0e}    {measured}        {:.3}       {:.2}%{}",
            score_thr(far),
            frr_at(score_thr(far)) * 100.0,
            if far < vlow { "  (extrapolated)" } else { "" }
        );
    }
    println!("  threshold 0.50 -> fitted FAR {:.1e}, FRR {:.2}%", 10f64.powf(vi + vs * 0.50), frr_at(0.50) * 100.0);

    let ref_mean = users.iter().map(|u| u.cohort_mean).sum::<f64>() / users.len() as f64;
    let ref_std = users.iter().map(|u| u.cohort_std).sum::<f64>() / users.len() as f64;
    println!(
        "\n== personal thresholds: cohort mean {ref_mean:.4} (per-user sd {:.4}), cohort std {ref_std:.4} (per-user sd {:.4})",
        mean_std(&users.iter().map(|u| u.cohort_mean).collect::<Vec<_>>()).1,
        mean_std(&users.iter().map(|u| u.cohort_std).collect::<Vec<_>>()).1
    );
    let ref_q99 = users.iter().map(|u| u.cohort_q99).sum::<f64>() / users.len() as f64;
    let z_of = |t: f64| (t - ref_mean) / ref_std;
    let personal = |u: &Enrollee, t: f64| u.cohort_mean + z_of(t) * u.cohort_std;
    let pct = |v: &mut Vec<f64>, q: f64| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() - 1) as f64 * q).round() as usize]
    };
    println!("  pooled FAR | rule        | FRR     | per-user FAR p50 / p90 / max");
    let mut summary = Vec::new();
    for target in [1e-3, 1e-4, 3e-5] {
        let rules: [(&str, Rule); 4] = [
            ("global", Box::new(|_, t| t)),
            ("z-norm", Box::new(personal)),
            ("z-raise-only", Box::new(|u, t| t.max(personal(u, t)).min(t + 0.10))),
            ("q99-shift", Box::new(move |u, t| t + (u.cohort_q99 - ref_q99))),
        ];
        for (name, rule) in &rules {
            let (param, far, frr, mut per) = solve(&users, target, 0.0, 0.9, rule.as_ref());
            println!(
                "  {target:.0e}      | {name:<11} | {:>5.2}%  | {:.1e} / {:.1e} / {:.1e}  sd {:.1e}  (base {param:.3})",
                frr * 100.0,
                pct(&mut per, 0.5),
                pct(&mut per, 0.9),
                pct(&mut per, 1.0),
                mean_std(&per).1
            );
            summary.push(serde_json::json!({"targetFar": target, "rule": name, "base": param, "far": far, "frr": frr,
                "perUserFarP90": pct(&mut per, 0.9), "perUserFarMax": pct(&mut per, 1.0)}));
        }
    }
    let t3 = score_tail.threshold_at(1e-3);
    let actual: Vec<f64> = users
        .iter()
        .map(|u| u.impostor.iter().filter(|&&s| f64::from(s) >= t3).count() as f64 / u.impostor.len() as f64)
        .collect();
    let predicted: Vec<f64> = users.iter().map(|u| -(t3 - u.cohort_mean) / u.cohort_std).collect();
    let (ma, sa) = mean_std(&actual);
    let (mp, sp) = mean_std(&predicted);
    let corr =
        actual.iter().zip(&predicted).map(|(a, p)| (a - ma) * (p - mp)).sum::<f64>() / (actual.len() as f64 * sa * sp);
    println!("  correlation(cohort-predicted risk, actual per-user FAR at global 1e-3 threshold) = {corr:.2}");

    let mut rs = std::fs::File::create(out.join("calibration_data.rs")).unwrap();
    writeln!(rs, "pub const MEASURED_RECOGNIZER: &str = {RECOGNIZER_ID:?};").unwrap();
    writeln!(rs, "pub const SCORE_TAIL_INTERCEPT: f64 = {};", literal(vi)).unwrap();
    writeln!(rs, "pub const SCORE_TAIL_SLOPE: f64 = {};", literal(vs)).unwrap();
    writeln!(rs, "pub const SCORE_TAIL_MEASURED_FAR: f64 = {vlow:.3e};").unwrap();
    writeln!(rs, "pub const PAIR_TAIL_INTERCEPT: f64 = {};", literal(pi)).unwrap();
    writeln!(rs, "pub const PAIR_TAIL_SLOPE: f64 = {};", literal(ps)).unwrap();
    writeln!(rs, "pub const PAIR_TAIL_MEASURED_FAR: f64 = {plow:.3e};").unwrap();

    let report = serde_json::json!({
        "images": samples.len(), "identities": by_id.len(), "cohort": cohort.len(), "enrollees": users.len(),
        "impostorTrials": score_tail.total, "genuineTrials": n_gen,
        "scoreTail": {"intercept": vi, "slope": vs, "measuredDownTo": vlow},
        "pairTail": {"intercept": pi, "slope": ps, "measuredDownTo": plow},
        "personalVsGlobal": summary, "riskCorrelation": corr,
    });
    std::fs::write(out.join("calibration.json"), serde_json::to_string_pretty(&report).unwrap()).unwrap();
}
