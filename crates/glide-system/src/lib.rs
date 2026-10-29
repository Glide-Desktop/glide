pub mod cursor_config;
pub mod spawn;
pub mod status;
pub mod status_hooks;
pub mod system_info;
pub mod theme_export;

use glide_shell::i18n;
pub use spawn::{reap_child_processes, spawn_detached, spawn_shell_command, spawn_shell_command_with_env};
