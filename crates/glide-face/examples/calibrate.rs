#[path = "calibrate/analyze.rs"]
mod analyze;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use glide_face::biometrics::align::align_face_mesh;
use glide_face::biometrics::landmark_onnx::Landmark68Aligner;
use glide_face::biometrics::recognizer::FaceRecognizer;
use glide_face::biometrics::yunet::YuNetDetector;
use glide_face::biometrics::{DetectedFace, EMBEDDING_DIM, RgbaImage, estimate_head_pose_3d};

pub struct Sample {
    pub identity: String,
    pub yaw: f32,
    pub face_width: f32,
    pub emb: Vec<f32>,
}

#[must_use]
pub fn load_cache(dir: &Path) -> Vec<Sample> {
    let meta = std::fs::read_to_string(dir.join("meta.tsv")).expect("meta.tsv");
    let raw = std::fs::read(dir.join("embeddings.f32")).expect("embeddings.f32");
    let dim = EMBEDDING_DIM;
    meta.lines()
        .enumerate()
        .map(|(i, line)| {
            let f: Vec<&str> = line.split('\t').collect();
            let emb = raw[i * dim * 4..(i + 1) * dim * 4]
                .chunks_exact(4)
                .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
                .collect();
            Sample { identity: f[0].into(), yaw: f[1].parse().unwrap(), face_width: f[2].parse().unwrap(), emb }
        })
        .collect()
}

fn embed(root: &Path, out: &Path) {
    let mut images: Vec<(String, PathBuf)> = Vec::new();
    for person in std::fs::read_dir(root).unwrap().flatten() {
        let name = person.file_name().to_string_lossy().into_owned();
        for img in std::fs::read_dir(person.path()).into_iter().flatten().flatten() {
            images.push((name.clone(), img.path()));
        }
    }
    images.sort();
    eprintln!("{} images", images.len());
    let images = Arc::new(images);
    let next = Arc::new(AtomicUsize::new(0));
    let results: Arc<Mutex<Vec<(usize, Sample)>>> = Arc::new(Mutex::new(Vec::new()));
    let workers: Vec<_> = (0..5)
        .map(|_| {
            let (next, results, images) = (next.clone(), results.clone(), images.clone());
            std::thread::spawn(move || {
                let det = YuNetDetector::new().unwrap();
                let lm = Landmark68Aligner::new().unwrap();
                let rec = FaceRecognizer::new().unwrap();
                loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    let Some((name, path)) = images.get(i) else { break };
                    if i % 2000 == 0 {
                        eprintln!("  {i}/{}", images.len());
                    }
                    let Ok(img) = RgbaImage::open(path) else { continue };
                    let mut faces = det.detect(&img).unwrap_or_default();
                    let (cx, cy) = (img.width as f32 / 2.0, img.height as f32 / 2.0);
                    let d = |f: &DetectedFace| (f.x + f.width / 2.0 - cx).hypot(f.y + f.height / 2.0 - cy);
                    faces.sort_by(|a, b| d(a).partial_cmp(&d(b)).unwrap());
                    let Some(mut face) = faces.into_iter().next() else { continue };
                    let Ok(dense) = lm.predict68(&img, &mut face) else { continue };
                    face.dense = dense;
                    let (yaw, _, _, _) = estimate_head_pose_3d(&face);
                    let emb = rec.extract_embedding(&align_face_mesh(&img, &face)).unwrap();
                    let s = Sample { identity: name.clone(), yaw: yaw as f32, face_width: face.width, emb };
                    results.lock().unwrap().push((i, s));
                }
            })
        })
        .collect();
    for worker in workers {
        worker.join().unwrap();
    }
    let mut results = std::mem::take(&mut *results.lock().unwrap());
    results.sort_by_key(|r| r.0);
    std::fs::create_dir_all(out).unwrap();
    let mut meta = std::fs::File::create(out.join("meta.tsv")).unwrap();
    let mut raw = std::io::BufWriter::new(std::fs::File::create(out.join("embeddings.f32")).unwrap());
    for (_, s) in &results {
        writeln!(meta, "{}\t{:.2}\t{:.1}", s.identity, s.yaw, s.face_width).unwrap();
        for v in &s.emb {
            raw.write_all(&v.to_le_bytes()).unwrap();
        }
    }
    eprintln!("embedded {} of {} images", results.len(), images.len());
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("embed") if args.len() == 3 => embed(Path::new(&args[1]), Path::new(&args[2])),
        Some("analyze") if args.len() == 3 => analyze::run(&load_cache(Path::new(&args[1])), Path::new(&args[2])),
        _ => eprintln!("usage: calibrate embed <lfw_dir> <cache_dir> | calibrate analyze <cache_dir> <out_dir>"),
    }
}
