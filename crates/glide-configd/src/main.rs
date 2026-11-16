use std::os::unix::fs::PermissionsExt;
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use glide_configd::store::{Defaults, Store};

fn main() -> ExitCode {
    tracing_subscriber::fmt().with_env_filter(tracing_subscriber::EnvFilter::from_default_env()).init();
    let Some(path) = glide_configd::database_path() else {
        tracing::error!("neither XDG_CONFIG_HOME nor HOME is set");
        return ExitCode::FAILURE;
    };
    if let Some(dir) = path.parent()
        && let Err(err) = std::fs::create_dir_all(dir)
    {
        tracing::error!("cannot create {}: {err}", dir.display());
        return ExitCode::FAILURE;
    }
    let mut store = match Store::open(&path, Defaults::load(&glide_configd::defaults_dirs())) {
        Ok(store) => store,
        Err(err) => {
            tracing::error!("{err}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(err) = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)) {
        tracing::warn!("cannot make {} private: {err}", path.display());
    }
    if let Some(legacy) = glide_config::xdg_path("glide", "glide.toml") {
        glide_configd::import::legacy_file(&mut store, glide_ipc::config::SHELL, &legacy);
    }
    let trusted = glide_configd::bus::trusted_next_to(std::env::current_exe().ok().as_deref());
    let builder = match zbus::blocking::connection::Builder::session() {
        Ok(builder) => builder,
        Err(err) => {
            tracing::error!("no session bus: {err}");
            return ExitCode::FAILURE;
        }
    };
    let _connection = match glide_configd::bus::serve_on(builder, Arc::new(Mutex::new(store)), trusted) {
        Ok(connection) => connection,
        Err(err) => {
            tracing::error!("cannot serve {}: {err}", glide_configd::bus::BUS_NAME);
            return ExitCode::FAILURE;
        }
    };
    loop {
        std::thread::park();
    }
}
