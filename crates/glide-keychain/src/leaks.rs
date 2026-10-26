use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::Duration;

use sha1::{Digest, Sha1};

pub const RANGE_URL: &str = "https://api.pwnedpasswords.com/range/";
pub const RECHECK_AFTER: u64 = 24 * 3600;
const PREFIX: usize = 5;

#[must_use]
pub fn sha1_hex(password: &str) -> String {
    data_encoding::HEXUPPER.encode(&Sha1::digest(password.as_bytes()))
}

#[must_use]
pub fn leaked_suffixes(body: &str) -> HashSet<String> {
    body.lines()
        .filter_map(|line| line.trim().split_once(':'))
        .filter(|(_, count)| count.trim().parse::<u64>().is_ok_and(|count| count > 0))
        .map(|(suffix, _)| suffix.trim().to_ascii_uppercase())
        .collect()
}

pub fn check(
    hashes: &[String], mut fetch: impl FnMut(&str) -> Result<String, String>,
) -> Result<HashMap<String, bool>, String> {
    let mut by_prefix: BTreeMap<&str, Vec<&String>> = BTreeMap::new();
    for hash in hashes.iter().filter(|hash| hash.len() > PREFIX) {
        by_prefix.entry(&hash[..PREFIX]).or_default().push(hash);
    }
    let mut found = HashMap::new();
    for (prefix, group) in by_prefix {
        let leaked = leaked_suffixes(&fetch(prefix)?);
        for hash in group {
            found.insert(hash.clone(), leaked.contains(&hash[PREFIX..]));
        }
    }
    Ok(found)
}

pub fn fetch_range(prefix: &str) -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .user_agent(concat!("Glide-Passwords/", env!("CARGO_PKG_VERSION")))
        .build()
        .into();
    agent
        .get(&format!("{RANGE_URL}{prefix}"))
        .header("Add-Padding", "true")
        .call()
        .map_err(|err| err.to_string())?
        .body_mut()
        .read_to_string()
        .map_err(|err| err.to_string())
}

#[derive(Debug, Default)]
pub struct Leaks {
    known: HashMap<String, (bool, u64)>,
}

impl Leaks {
    #[must_use]
    pub fn stale<'a>(&self, hashes: impl Iterator<Item = &'a String>, now: u64) -> Vec<String> {
        let unique: HashSet<&String> = hashes.collect();
        let mut stale: Vec<String> = unique
            .into_iter()
            .filter(|hash| self.known.get(*hash).is_none_or(|(_, at)| now.saturating_sub(*at) >= RECHECK_AFTER))
            .cloned()
            .collect();
        stale.sort();
        stale
    }

    pub fn store(&mut self, found: HashMap<String, bool>, now: u64) {
        for (hash, leaked) in found {
            self.known.insert(hash, (leaked, now));
        }
    }

    #[must_use]
    pub fn leaked(&self, hash: &str) -> bool {
        self.known.get(hash).is_some_and(|(leaked, _)| *leaked)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hash_is_the_one_the_service_uses() {
        assert_eq!(sha1_hex("password"), "5BAA61E4C9B93F3F0682250B6CF8331B7EE68FD8");
    }

    #[test]
    fn padding_lines_with_a_zero_count_are_not_leaks() {
        let body = "1E4C9B93F3F0682250B6CF8331B7EE68FD8:9659365\r\n0018A45C4D1DEF81644B54AB7F969B88D65:0\r\n";
        let leaked = leaked_suffixes(body);
        assert!(leaked.contains("1E4C9B93F3F0682250B6CF8331B7EE68FD8"));
        assert_eq!(leaked.len(), 1);
    }

    #[test]
    fn only_the_five_character_prefix_leaves_and_once_per_prefix() {
        let hashes = vec![sha1_hex("password"), sha1_hex("a much longer passphrase nobody uses")];
        let mut asked = Vec::new();
        let found = check(&hashes, |prefix| {
            asked.push(prefix.to_string());
            Ok("1E4C9B93F3F0682250B6CF8331B7EE68FD8:3\n".into())
        })
        .expect("check");
        assert!(asked.iter().all(|prefix| prefix.len() == 5));
        assert_eq!(asked.len(), 2);
        assert!(found[&hashes[0]]);
        assert!(!found[&hashes[1]]);
    }

    #[test]
    fn results_are_kept_for_a_day() {
        let hash = sha1_hex("password");
        let mut leaks = Leaks::default();
        assert_eq!(leaks.stale([&hash].into_iter(), 100), std::slice::from_ref(&hash));
        leaks.store(HashMap::from([(hash.clone(), true)]), 100);
        assert!(leaks.stale([&hash].into_iter(), 100 + RECHECK_AFTER - 1).is_empty());
        assert_eq!(leaks.stale([&hash].into_iter(), 100 + RECHECK_AFTER).len(), 1);
        assert!(leaks.leaked(&hash));
    }
}
