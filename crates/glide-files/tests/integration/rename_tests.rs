use std::path::{Path, PathBuf};

use glide_files::fs::rename::{Problem, Rule, apply, counts, plan, problem, renamed, split_name};

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("glide-rename-{tag}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn names(dir: &Path) -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(dir)
        .expect("listing")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

fn taken(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok()
}

#[test]
fn names_split_at_the_last_dot_except_archives_folders_and_dotfiles() {
    assert_eq!(split_name("звіт.final.pdf", false), ("звіт.final", ".pdf"));
    assert_eq!(split_name("backup.tar.gz", false), ("backup", ".tar.gz"));
    assert_eq!(split_name(".bashrc", false), (".bashrc", ""));
    assert_eq!(split_name("trailing.", false), ("trailing.", ""));
    assert_eq!(split_name("v1.2", true), ("v1.2", ""));
}

#[test]
fn a_template_numbers_items_and_keeps_each_extension() {
    let rule = Rule::Template(String::from("Фото [001]"));
    assert_eq!(renamed("IMG_1.jpg", false, &rule, 0), "Фото 001.jpg");
    assert_eq!(renamed("IMG_2.png", false, &rule, 11), "Фото 012.png");
    let rule = Rule::Template(String::from("[N] — копія [5]"));
    assert_eq!(renamed("a.tar.gz", false, &rule, 2), "a — копія 7.tar.gz");
    assert_eq!(renamed("теки", true, &Rule::Template(String::from("[n]_[x]")), 0), "теки_[x]");
    assert!(counts("x [01]"));
    assert!(!counts("x [N] [a1]"));
}

#[test]
fn find_and_replace_touches_every_occurrence_of_the_whole_name() {
    let rule = Rule::Replace { find: String::from("IMG"), with: String::from("Фото") };
    assert_eq!(renamed("IMG_IMG.jpg", false, &rule, 0), "Фото_Фото.jpg");
    let idle = Rule::Replace { find: String::new(), with: String::from("x") };
    assert_eq!(renamed("a.txt", false, &idle, 0), "a.txt");
}

#[test]
fn a_plan_is_refused_when_names_repeat_are_taken_invalid_or_unchanged() {
    let root = scratch("problems");
    for name in ["a.txt", "b.txt", "other.txt"] {
        std::fs::write(root.join(name), name).expect("file");
    }
    let paths = vec![(root.join("a.txt"), false), (root.join("b.txt"), false)];
    let check = |rule: Rule| problem(&plan(&paths, &rule), &rule, taken);
    assert_eq!(check(Rule::Template(String::from("same"))), Some(Problem::Repeats));
    assert_eq!(check(Rule::Template(String::from("  "))), Some(Problem::Empty));
    assert_eq!(check(Rule::Template(String::from("a/[1]"))), Some(Problem::Invalid));
    assert_eq!(check(Rule::Replace { find: String::from("zzz"), with: String::new() }), Some(Problem::Unchanged));
    assert_eq!(check(Rule::Replace { find: String::from("a"), with: String::from("other") }), Some(Problem::Taken));
    assert_eq!(check(Rule::Replace { find: String::from(".txt"), with: String::new() }), None);
    assert_eq!(check(Rule::Template(String::from("doc [1]"))), None);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn applying_a_plan_renames_everything_even_when_names_swap() {
    let root = scratch("apply");
    for name in ["1.txt", "2.txt", "3.txt"] {
        std::fs::write(root.join(name), name).expect("file");
    }
    let paths = vec![(root.join("1.txt"), false), (root.join("2.txt"), false), (root.join("3.txt"), false)];
    let rule = Rule::Template(String::from("[2]"));
    let planned = plan(&paths, &rule);
    assert_eq!(problem(&planned, &rule, taken), None, "2 and 3 are taken only by items of the same plan");
    let (done, failure) = apply(&planned);
    assert!(failure.is_none());
    assert_eq!(done.len(), 3);
    assert_eq!(names(&root), ["2.txt", "3.txt", "4.txt"]);
    assert_eq!(std::fs::read_to_string(root.join("4.txt")).expect("moved"), "3.txt");
    let back: Vec<(PathBuf, PathBuf)> = done.into_iter().map(|(from, to)| (to, from)).collect();
    let (undone, failure) = apply(&back);
    assert!(failure.is_none());
    assert_eq!(undone.len(), 3);
    assert_eq!(names(&root), ["1.txt", "2.txt", "3.txt"]);
    assert_eq!(std::fs::read_to_string(root.join("1.txt")).expect("restored"), "1.txt");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_name_that_appears_meanwhile_stops_the_rename_without_overwriting_it() {
    let root = scratch("race");
    for name in ["a.txt", "b.txt"] {
        std::fs::write(root.join(name), name).expect("file");
    }
    let planned = vec![(root.join("a.txt"), root.join("x.txt")), (root.join("b.txt"), root.join("y.txt"))];
    std::fs::write(root.join("y.txt"), "kept").expect("intruder");
    let (done, failure) = apply(&planned);
    assert_eq!(done, [(root.join("a.txt"), root.join("x.txt"))]);
    assert_eq!(failure.map(|err| err.kind()), Some(std::io::ErrorKind::AlreadyExists));
    assert_eq!(names(&root), ["b.txt", "x.txt", "y.txt"]);
    assert_eq!(std::fs::read_to_string(root.join("y.txt")).expect("kept"), "kept");
    let _ = std::fs::remove_dir_all(root);
}
