use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: PathBuf,
    pub name: String,
    pub folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub entry: Entry,
    pub depth: usize,
    pub expanded: bool,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tree {
    pub root: PathBuf,
    pub nodes: Vec<Node>,
    pub loaded: bool,
}

const SKIPPED: [&str; 3] = [".git", "node_modules", "target"];

pub fn list(folder: &Path, hidden: bool) -> std::io::Result<Vec<Entry>> {
    let mut entries: Vec<Entry> = std::fs::read_dir(folder)?
        .filter_map(Result::ok)
        .filter_map(|item| {
            let name = item.file_name().to_string_lossy().into_owned();
            if !hidden && name.starts_with('.') {
                return None;
            }
            let path = item.path();
            let folder = std::fs::metadata(&path).is_ok_and(|meta| meta.is_dir());
            if folder && !hidden && SKIPPED.contains(&name.as_str()) {
                return None;
            }
            Some(Entry { path, name, folder })
        })
        .collect();
    entries.sort_by(|a, b| b.folder.cmp(&a.folder).then_with(|| natural(&a.name, &b.name)));
    Ok(entries)
}

fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let key = |name: &str| {
        let mut parts: Vec<(String, u64)> = Vec::new();
        let mut text = String::new();
        let mut number = String::new();
        for c in name.chars() {
            if c.is_ascii_digit() {
                number.push(c);
            } else {
                if !number.is_empty() {
                    parts.push((std::mem::take(&mut text), number.parse().unwrap_or(u64::MAX)));
                    number.clear();
                }
                text.extend(c.to_lowercase());
            }
        }
        parts.push((text, number.parse().unwrap_or(0)));
        parts
    };
    key(a).cmp(&key(b)).then_with(|| a.cmp(b))
}

impl Tree {
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root, nodes: Vec::new(), loaded: false }
    }

    #[must_use]
    pub fn title(&self) -> String {
        self.root
            .file_name()
            .map_or_else(|| self.root.display().to_string(), |name| name.to_string_lossy().into_owned())
    }

    fn position(&self, path: &Path) -> Option<usize> {
        self.nodes.iter().position(|node| node.entry.path == path)
    }

    fn subtree_end(&self, index: usize) -> usize {
        let depth = self.nodes[index].depth;
        self.nodes[index + 1..]
            .iter()
            .position(|node| node.depth <= depth)
            .map_or(self.nodes.len(), |at| index + 1 + at)
    }

    pub fn fill(&mut self, folder: &Path, entries: Vec<Entry>) {
        if folder == self.root {
            let previous = std::mem::take(&mut self.nodes);
            let mut nodes = Vec::with_capacity(previous.len().max(entries.len()));
            for entry in entries {
                let at = previous.iter().position(|node| node.depth == 0 && node.entry.path == entry.path);
                match at.filter(|&at| previous[at].expanded) {
                    Some(at) => {
                        let end = previous[at + 1..]
                            .iter()
                            .position(|node| node.depth == 0)
                            .map_or(previous.len(), |offset| at + 1 + offset);
                        nodes.push(Node { entry, depth: 0, expanded: true, loading: true });
                        nodes.extend(previous[at + 1..end].iter().cloned());
                    }
                    None => nodes.push(Node { entry, depth: 0, expanded: false, loading: false }),
                }
            }
            self.nodes = nodes;
            self.loaded = true;
            return;
        }
        let Some(index) = self.position(folder) else { return };
        if !self.nodes[index].expanded {
            self.nodes[index].loading = false;
            return;
        }
        let end = self.subtree_end(index);
        let depth = self.nodes[index].depth + 1;
        let previously: Vec<PathBuf> = self.nodes[index + 1..end]
            .iter()
            .filter(|node| node.expanded)
            .map(|node| node.entry.path.clone())
            .collect();
        let children: Vec<Node> = entries
            .into_iter()
            .map(|entry| {
                let expanded = previously.contains(&entry.path);
                Node { entry, depth, expanded, loading: expanded }
            })
            .collect();
        self.nodes.splice(index + 1..end, children);
        self.nodes[index].loading = false;
    }

    pub fn toggle(&mut self, path: &Path) -> Option<PathBuf> {
        let index = self.position(path)?;
        if !self.nodes[index].entry.folder {
            return None;
        }
        if self.nodes[index].expanded {
            let end = self.subtree_end(index);
            self.nodes.drain(index + 1..end);
            self.nodes[index].expanded = false;
            self.nodes[index].loading = false;
            return None;
        }
        self.nodes[index].expanded = true;
        self.nodes[index].loading = true;
        Some(path.to_path_buf())
    }

    #[must_use]
    pub fn expanded(&self) -> Vec<PathBuf> {
        self.nodes.iter().filter(|node| node.expanded).map(|node| node.entry.path.clone()).collect()
    }

    #[must_use]
    pub fn pending(&self) -> Vec<PathBuf> {
        self.nodes.iter().filter(|node| node.loading).map(|node| node.entry.path.clone()).collect()
    }

    pub fn reveal(&mut self, path: &Path) -> Vec<PathBuf> {
        let Ok(relative) = path.strip_prefix(&self.root) else { return Vec::new() };
        let mut wanted = Vec::new();
        let mut current = self.root.clone();
        let components: Vec<_> = relative.components().collect();
        for component in components.iter().take(components.len().saturating_sub(1)) {
            current.push(component);
            match self.position(&current) {
                Some(index) if !self.nodes[index].expanded => {
                    if let Some(folder) = self.toggle(&current.clone()) {
                        wanted.push(folder);
                    }
                }
                Some(_) => {}
                None => break,
            }
        }
        wanted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, folder: bool) -> Entry {
        let path = PathBuf::from(path);
        Entry { name: path.file_name().unwrap().to_string_lossy().into_owned(), path, folder }
    }

    #[test]
    fn folders_come_first_in_natural_order_and_hidden_files_stay_hidden() {
        let root = std::env::temp_dir().join(format!("glide-text-tree-{}", std::process::id()));
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(root.join(".git")).unwrap();
        for name in ["file10.txt", "file2.txt", "B.md", ".hidden"] {
            std::fs::write(root.join(name), "").unwrap();
        }
        let names: Vec<String> = list(&root, false).unwrap().into_iter().map(|entry| entry.name).collect();
        assert_eq!(names, ["src", "B.md", "file2.txt", "file10.txt"]);
        assert_eq!(list(&root, true).unwrap().len(), 6);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn expanding_inserts_children_below_their_folder_and_collapsing_removes_them() {
        let mut tree = Tree::new(PathBuf::from("/r"));
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true), entry("/r/z.txt", false)]);
        assert_eq!(tree.toggle(&PathBuf::from("/r/a")), Some(PathBuf::from("/r/a")));
        tree.fill(&PathBuf::from("/r/a"), vec![entry("/r/a/b", true), entry("/r/a/c.rs", false)]);
        let order: Vec<_> = tree.nodes.iter().map(|node| (node.entry.name.as_str(), node.depth)).collect();
        assert_eq!(order, [("a", 0), ("b", 1), ("c.rs", 1), ("z.txt", 0)]);
        tree.toggle(&PathBuf::from("/r/a/b"));
        tree.fill(&PathBuf::from("/r/a/b"), vec![entry("/r/a/b/d", false)]);
        assert_eq!(tree.nodes.len(), 5);
        assert_eq!(tree.toggle(&PathBuf::from("/r/a")), None);
        assert_eq!(tree.nodes.len(), 2);
    }

    #[test]
    fn a_refresh_keeps_open_folders_open() {
        let mut tree = Tree::new(PathBuf::from("/r"));
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true)]);
        tree.toggle(&PathBuf::from("/r/a"));
        tree.fill(&PathBuf::from("/r/a"), vec![entry("/r/a/x", false)]);
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true), entry("/r/new", false)]);
        assert_eq!(tree.pending(), vec![PathBuf::from("/r/a")]);
        assert!(tree.nodes[0].expanded);
    }

    #[test]
    fn collapsing_a_folder_that_is_still_loading_leaves_nothing_pending() {
        let mut tree = Tree::new(PathBuf::from("/r"));
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true), entry("/r/b", true)]);
        assert_eq!(tree.toggle(&PathBuf::from("/r/a")), Some(PathBuf::from("/r/a")));
        assert_eq!(tree.pending(), vec![PathBuf::from("/r/a")]);
        assert_eq!(tree.toggle(&PathBuf::from("/r/a")), None);
        assert!(tree.pending().is_empty(), "a collapsed folder is not waiting for a listing");
        tree.fill(&PathBuf::from("/r/a"), vec![entry("/r/a/x", false)]);
        assert_eq!(tree.nodes.len(), 2, "the late listing of a collapsed folder adds nothing");
        assert!(tree.pending().is_empty());
    }

    #[test]
    fn two_folders_collapsed_while_loading_do_not_ask_for_each_other_forever() {
        let a = PathBuf::from("/r/a");
        let b = PathBuf::from("/r/b");
        let mut tree = Tree::new(PathBuf::from("/r"));
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true), entry("/r/b", true)]);
        for folder in [&a, &b, &a, &b] {
            tree.toggle(folder);
        }
        let mut relisted = 0;
        for arrived in [&a, &b] {
            tree.fill(arrived, vec![entry("/r/n", false)]);
            relisted += tree.pending().into_iter().filter(|path| path != arrived).count();
        }
        assert_eq!(relisted, 0, "no listing may trigger another one while both folders are collapsed");
    }

    #[test]
    fn a_listing_that_arrives_for_a_collapsed_folder_clears_its_loading_flag() {
        let mut tree = Tree::new(PathBuf::from("/r"));
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true)]);
        tree.nodes[0].loading = true;
        tree.fill(&PathBuf::from("/r/a"), vec![entry("/r/a/x", false)]);
        assert!(!tree.nodes[0].loading);
        assert_eq!(tree.nodes.len(), 1);
    }

    #[test]
    fn revealing_a_file_opens_every_folder_above_it() {
        let mut tree = Tree::new(PathBuf::from("/r"));
        tree.fill(&PathBuf::from("/r"), vec![entry("/r/a", true)]);
        assert_eq!(tree.reveal(&PathBuf::from("/r/a/b/c.txt")), vec![PathBuf::from("/r/a")]);
        assert!(tree.reveal(&PathBuf::from("/elsewhere/c.txt")).is_empty());
    }
}
