use std::path::{Path, PathBuf};

#[must_use]
pub fn executable(compositor: Option<&Path>) -> PathBuf {
    compositor
        .and_then(Path::parent)
        .map(|directory| directory.join(glide_config::welcome::APPLICATION_ID))
        .filter(|candidate| candidate.is_file())
        .unwrap_or_else(|| PathBuf::from(glide_config::welcome::APPLICATION_ID))
}

pub fn launch_first_run(marker: &Path, start: impl FnOnce() -> std::io::Result<u32>) -> std::io::Result<Option<u32>> {
    if glide_config::welcome::first_run(marker) { start().map(Some) } else { Ok(None) }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    #[test]
    fn a_new_account_retries_until_the_tour_completes() {
        let root = std::env::temp_dir().join(format!("glide-welcome-launch-{}", std::process::id()));
        let marker = root.join("glide/welcomed");
        let starts = Cell::new(0);
        let launch = || {
            starts.set(starts.get() + 1);
            Ok(42)
        };
        assert_eq!(launch_first_run(&marker, launch).expect("first launch"), Some(42));
        assert!(glide_config::welcome::first_run(&marker));
        assert_eq!(launch_first_run(&marker, launch).expect("unfinished tour is offered again"), Some(42));
        glide_config::welcome::mark_complete(&marker).expect("tour is completed");
        assert_eq!(launch_first_run(&marker, launch).expect("completed tour is skipped"), None);
        assert_eq!(starts.get(), 2);
        std::fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn failed_launch_does_not_mark_the_tour_complete() {
        let root = std::env::temp_dir().join(format!("glide-welcome-launch-failed-{}", std::process::id()));
        let marker = root.join("glide/welcomed");
        let result = launch_first_run(&marker, || Err(std::io::Error::other("missing executable")));
        assert!(result.is_err());
        assert!(glide_config::welcome::first_run(&marker));
    }

    #[test]
    fn welcome_is_found_beside_the_installed_compositor_or_through_path() {
        let root = std::env::temp_dir().join(format!("glide-welcome-executable-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("create installation directory");
        let compositor = root.join("glide");
        let welcome = root.join(glide_config::welcome::APPLICATION_ID);
        assert_eq!(executable(Some(&compositor)), PathBuf::from(glide_config::welcome::APPLICATION_ID));
        std::fs::write(&welcome, b"").expect("create welcome executable");
        assert_eq!(executable(Some(&compositor)), welcome);
        assert_eq!(executable(None), PathBuf::from(glide_config::welcome::APPLICATION_ID));
        std::fs::remove_dir_all(root).expect("cleanup");
    }
}
