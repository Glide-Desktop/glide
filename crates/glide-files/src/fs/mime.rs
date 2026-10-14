use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub const DIRECTORY: &str = "inode/directory";
pub const OCTET_STREAM: &str = "application/octet-stream";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MimeDb {
    literals: HashMap<String, String>,
    suffixes: HashMap<String, String>,
    suffixes_cs: HashMap<String, String>,
    globs: Vec<(String, String)>,
    parents: HashMap<String, Vec<String>>,
    aliases: HashMap<String, String>,
    icons: HashMap<String, String>,
    generic_icons: HashMap<String, String>,
}

fn has_wildcard(glob: &str) -> bool {
    glob.contains(['*', '?', '['])
}

impl MimeDb {
    #[must_use]
    pub fn load(data_dirs: &[PathBuf]) -> Self {
        let mut db = Self::default();
        for dir in data_dirs.iter().map(|dir| dir.join("mime")) {
            let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap_or_default();
            db.add_globs(&read("globs2"));
            for (child, parent) in pairs(&read("subclasses")) {
                let parents = db.parents.entry(child).or_default();
                if !parents.contains(&parent) {
                    parents.push(parent);
                }
            }
            for (alias, canonical) in pairs(&read("aliases")) {
                db.aliases.entry(alias).or_insert(canonical);
            }
            for (map, file) in [(&mut db.icons, "icons"), (&mut db.generic_icons, "generic-icons")] {
                for (mime, icon) in colon_pairs(&read(file)) {
                    map.entry(mime).or_insert(icon);
                }
            }
        }
        db
    }

    pub fn add_globs(&mut self, globs2: &str) {
        for line in globs2.lines().filter(|line| !line.starts_with('#')) {
            let mut fields = line.split(':').skip(1);
            let (Some(mime), Some(glob)) = (fields.next(), fields.next()) else {
                continue;
            };
            let case_sensitive = fields.next().is_some_and(|flags| flags.split(',').any(|flag| flag == "cs"));
            let mime = mime.to_string();
            let slot = match glob.strip_prefix('*').filter(|rest| !has_wildcard(rest)) {
                Some(suffix) if case_sensitive => self.suffixes_cs.entry(suffix.to_string()),
                Some(suffix) => self.suffixes.entry(suffix.to_lowercase()),
                None if !has_wildcard(glob) => self.literals.entry(glob.to_lowercase()),
                None => {
                    self.globs.push((glob.to_string(), mime));
                    continue;
                }
            };
            slot.or_insert(mime);
        }
    }

    #[must_use]
    pub fn guess(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        if let Some(mime) = self.literals.get(&lower) {
            return Some(mime);
        }
        let suffix = name.char_indices().find_map(|(index, _)| {
            let tail = &name[index..];
            self.suffixes_cs.get(tail).or_else(|| self.suffixes.get(&tail.to_lowercase()))
        });
        suffix
            .or_else(|| {
                self.globs
                    .iter()
                    .find(|(glob, _)| glob_match(glob, name) || glob_match(glob, &lower))
                    .map(|(_, mime)| mime)
            })
            .map(String::as_str)
    }

    #[must_use]
    pub fn of(&self, path: &Path, is_dir: bool) -> String {
        if is_dir {
            return DIRECTORY.to_string();
        }
        let name = path.file_name().map(|name| name.to_string_lossy()).unwrap_or_default();
        self.canonical(self.guess(&name).unwrap_or(OCTET_STREAM)).to_string()
    }

    #[must_use]
    pub fn icon_names(&self, mime: &str) -> Vec<String> {
        let mut names = Vec::with_capacity(3);
        if let Some(icon) = self.icons.get(mime) {
            names.push(icon.clone());
        }
        names.push(mime.replace('/', "-"));
        let generic = self
            .generic_icons
            .get(mime)
            .cloned()
            .unwrap_or_else(|| format!("{}-x-generic", mime.split('/').next().unwrap_or_default()));
        names.push(generic);
        names
    }

    #[must_use]
    pub fn canonical<'a>(&'a self, mime: &'a str) -> &'a str {
        self.aliases.get(mime).map_or(mime, String::as_str)
    }

    #[must_use]
    pub fn lineage(&self, mime: &str) -> Vec<String> {
        let mut seen = vec![self.canonical(mime).to_string()];
        let mut index = 0;
        while let Some(current) = seen.get(index).cloned() {
            let mut next = self.parents.get(&current).cloned().unwrap_or_default();
            if current.starts_with("text/") {
                next.push("text/plain".to_string());
            }
            if !current.starts_with("inode/") {
                next.push(OCTET_STREAM.to_string());
            }
            for parent in next {
                let parent = self.canonical(&parent).to_string();
                if !seen.contains(&parent) {
                    seen.push(parent);
                }
            }
            index += 1;
        }
        seen
    }

    #[must_use]
    pub fn is_a(&self, mime: &str, pattern: &str) -> bool {
        match pattern {
            "*" | "*/*" | "all/all" => return true,
            "all/allfiles" => return mime != DIRECTORY,
            _ => {}
        }
        let pattern = self.canonical(pattern);
        let major = pattern.strip_suffix("/*");
        self.lineage(mime).iter().any(|ancestor| match major {
            Some(major) => ancestor.split('/').next() == Some(major),
            None => ancestor == pattern,
        })
    }
}

fn pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|line| {
        let (left, right) = line.trim().split_once(' ')?;
        Some((left.to_string(), right.trim().to_string()))
    })
}

fn colon_pairs(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|line| {
        let (left, right) = line.trim().split_once(':')?;
        Some((left.to_string(), right.trim().to_string()))
    })
}

#[must_use]
pub fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0, 0);
    let mut star = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}
