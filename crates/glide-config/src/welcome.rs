use std::path::{Path, PathBuf};

pub const APPLICATION_ID: &str = "glide-welcome";

#[must_use]
pub fn marker() -> Option<PathBuf> {
    Some(crate::state_home()?.join("glide").join("welcomed"))
}

#[must_use]
pub fn first_run(marker: &Path) -> bool {
    !marker.exists()
}

pub fn mark_complete(marker: &Path) -> std::io::Result<()> {
    crate::write_atomic(marker, b"", 0o600)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completion_marks_only_the_chosen_account_and_is_repeatable() {
        let root = std::env::temp_dir().join(format!("glide-welcome-state-{}", std::process::id()));
        let marker = root.join("glide/welcomed");
        let other = root.join("other/glide/welcomed");
        assert!(first_run(&marker));
        assert!(first_run(&other));
        mark_complete(&marker).expect("completion is written");
        mark_complete(&marker).expect("completion may be written again");
        assert!(!first_run(&marker));
        assert!(first_run(&other));
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn a_failed_write_does_not_create_a_completion_marker() {
        let root = std::env::temp_dir().join(format!("glide-welcome-not-a-directory-{}", std::process::id()));
        std::fs::write(&root, b"not a directory").expect("create file");
        let marker = root.join("glide/welcomed");
        assert!(mark_complete(&marker).is_err());
        assert!(first_run(&marker));
        std::fs::remove_file(root).expect("cleanup");
    }
}
