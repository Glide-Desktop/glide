use glide_config::welcome::marker;

pub const NOTICE_ID: u32 = u32::MAX - 8;
pub const OPEN_SHORTCUTS: &str = "open-shortcuts";

impl crate::GlideState {
    pub fn welcome_if_first_run(&mut self) {
        std::thread::spawn(|| {
            let Some(marker) = marker() else {
                return;
            };
            if let Err(err) = glide_system::welcome::launch_first_run(&marker, || {
                let compositor = std::env::current_exe().ok();
                let executable = glide_system::welcome::executable(compositor.as_deref());
                crate::spawn_detached(std::process::Command::new(executable))
            }) {
                tracing::warn!(%err, "could not launch the welcome application");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_welcome_is_shown_once_per_account() {
        let dir = std::env::temp_dir().join(format!("glide-welcome-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let marker = dir.join("glide").join("welcomed");
        assert!(glide_config::welcome::first_run(&marker));
        std::fs::create_dir_all(marker.parent().expect("marker has a parent")).expect("state directory is created");
        std::fs::write(&marker, b"").expect("the marker is written");
        assert!(!glide_config::welcome::first_run(&marker));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }
}
