use std::path::{Path, PathBuf};

fn violations(source: &str) -> Vec<usize> {
    let lines: Vec<&str> = source.lines().map(str::trim).collect();
    let mut found = Vec::new();
    let mut in_block = false;
    let mut in_raw = false;
    let mut justifying = false;
    for (index, line) in lines.iter().enumerate() {
        if in_raw || (line.contains("r\"") && !line.ends_with("\";")) {
            in_raw = !(in_raw && line.starts_with('"'));
            continue;
        }
        let comment = line.find("//").filter(|&at| !line[..at].contains('"')).map(|at| &line[at..]);
        let above_allow = line.starts_with("//")
            && lines[index..]
                .iter()
                .find(|next| !next.starts_with("//"))
                .is_some_and(|next| next.starts_with("#[allow(") || next.starts_with("#![allow("));
        let starts_note =
            comment.is_some_and(|comment| comment.starts_with("// SAFETY:")) || line.contains("allow(") || above_allow;
        justifying = starts_note || (justifying && line.starts_with("//"));
        let flagged = in_block || line.starts_with("/*") || (comment.is_some() && !justifying);
        if line.starts_with("/*") && !line.contains("*/") {
            in_block = true;
        } else if in_block && line.contains("*/") {
            in_block = false;
        }
        if flagged {
            found.push(index + 1);
        }
    }
    found
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        if path.is_dir() && path.file_name().is_some_and(|name| name != "target") {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn the_scanner_flags_prose_and_permits_safety_and_allow_notes() {
    let prose = "fn f() {\n    // prose\n    /// doc\n    /* block */\n    let x = 1;\n}\n";
    assert_eq!(violations(prose), [2, 3, 4]);
    let allowed = "// SAFETY: ok\n#[allow(dead_code)] // why\nlet s = \"http://x\";\n";
    assert!(violations(allowed).is_empty());
}

#[test]
fn no_rust_source_in_the_workspace_carries_explanatory_comments() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    rust_files(&crates, &mut files);
    let offenders: Vec<String> = files
        .iter()
        .filter_map(|file| {
            let lines = violations(&std::fs::read_to_string(file).ok()?);
            (!lines.is_empty()).then(|| format!("{}:{lines:?}", file.display()))
        })
        .collect();
    assert!(offenders.is_empty(), "{offenders:#?}");
}
