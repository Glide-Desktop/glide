#![allow(irrefutable_let_patterns)]

mod agent_prompt;
mod app_index;
mod app_menu;
mod battery_alert;
mod bluetooth_agent;
mod dock_glue;
mod drag_assist;
mod handlers;
mod launcher_entry;
mod logind;

mod autofill;
mod backend;
mod background_effect;
mod cast;
mod clip;
mod clipboard;
mod config;
mod config_repair;
mod cursor;
mod decoration;
mod disk_space;
mod dock;
mod dock_drop;
mod dock_preview;
mod focus;
mod focused_field;
mod force_quit;
mod framed;
mod global_menu;
mod grabs;
pub mod greeter;
mod handoff;
mod headset;
mod identity;
mod idle;
mod input;
mod ipc;
mod keychain;
mod keychain_prompt;
mod launcher;
mod layout_scope;
mod lid;
mod logging;
mod media_art;
mod mirror;
mod netbird;
mod nm_agent;
mod notifications;
mod oneshot;
mod osd;
mod otp;
mod overview;
mod paint;
mod passwords_applet;
mod polkit;
mod polkit_agent;
mod polkit_helper;
mod power;
mod power_dialog;
mod privilege;
mod qr_scan;
mod qs_net;
mod removable;
mod screen_picker;
mod screencopy;
mod screensaver;
mod screenshot_ui;
mod session_end;
mod shell_bus;
mod shortcut_guard;
mod shot;
mod smooth_wheel;
mod snap;
mod snapshot;
mod sound;
mod state;
mod status_hooks;
mod super_keys;
mod surface_fade;
mod switcher;
pub mod system_info;
#[cfg(test)]
mod test_bus;
mod theme_export;
mod tiling;
mod topbar;
mod tray;
mod welcome;
mod window;
mod x11_menus;
mod xdg_autostart;
mod xwayland;

use smithay::{
    backend::session::libseat::LibSeatSession,
    reexports::{
        calloop::{EventLoop, channel},
        wayland_server::Display,
    },
};

use backend::tty::TtyData;
use glide_shell::{i18n, icon, popup_menu, raster};
pub use state::GlideState;

const USAGE: &str = "\
Usage: glide [OPTION]

The Glide Wayland compositor and desktop shell. Run it from a free virtual
terminal or through the `glide-session` wrapper of a display manager.

Options:
      --greeter   run as a greetd login screen instead of a desktop session
  -h, --help      show this help and exit
  -V, --version   show the version and exit

Configuration is read from $XDG_CONFIG_HOME/glide/glide.toml; `glidectl`
talks to a running session.";

#[derive(Debug, PartialEq, Eq)]
enum Cli {
    Run,
    Help,
    Version,
    Unknown(String),
}

fn cli(args: impl Iterator<Item = String>) -> Cli {
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" => return Cli::Help,
            "-V" | "--version" => return Cli::Version,
            "--greeter" => {}
            _ => return Cli::Unknown(arg),
        }
    }
    Cli::Run
}

fn main() -> anyhow::Result<()> {
    match cli(std::env::args().skip(1)) {
        Cli::Run => {}
        Cli::Help => {
            println!("{USAGE}");
            return Ok(());
        }
        Cli::Version => {
            println!("glide {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Cli::Unknown(arg) => anyhow::bail!("unknown option `{arg}`\n\n{USAGE}"),
    }

    #[cfg(feature = "profile-with-tracy")]
    tracy_client::Client::start();
    profiling::register_thread!("Main");

    let log_path = logging::init();
    logging::install_panic_hook();

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        log = ?log_path,
        "starting glide"
    );

    if let Err(err) = run() {
        tracing::error!("fatal: {err:#}");
        return Err(err);
    }

    Ok(())
}

fn run() -> anyhow::Result<()> {
    let (session, notifier) = LibSeatSession::new()?;

    let mut event_loop = EventLoop::try_new()?;

    let display = Display::new()?;

    let mut state = GlideState::new(TtyData::new(session)?, event_loop.handle(), event_loop.get_signal(), display);

    state.handoff = handoff::Handoff::from_environment();

    let config = config::Config::load_or_default();
    config.apply_to_state(&mut state);

    crate::backend::tty::init_tty(&mut event_loop, &mut state, notifier)?;

    state.device_added()?;

    crate::backend::tty::init_udev_monitor(&mut event_loop, &mut state)?;

    let ipc_server = ipc::init_ipc(&mut event_loop, &state.socket_name)?;

    warn_about_a_foreign_session();
    apply_session_environment(&state.socket_name);
    if let Some(server) = ipc_server.as_ref() {
        // SAFETY: as in `apply_session_environment`: main thread, before the event loop runs.
        unsafe { std::env::set_var("GLIDE_IPC_SOCK", server.socket_path()) };
    }

    let is_greeter = std::env::args().any(|arg| arg == "--greeter")
        || (std::env::var("GREETD_SOCK").is_ok() && std::env::var("GLIDE_SESSION").is_err());
    let login_session = is_greeter || state.handoff.is_some();

    if is_greeter {
        tracing::info!("starting glide in greeter display manager mode");
        state.init_greeter(&mut event_loop)?;
        init_greeter_timer(&mut event_loop)?;
    } else {
        init_session_services(&mut event_loop, &mut state, &config)?;
    }

    event_loop.run(None, &mut state, move |state| {
        state.space.refresh();
        state.popups.cleanup();

        let login_animating = state.tick_login();
        let overview_animating = state.tick_overview();
        let hover_animating = state.any_button_hover_animating();
        let open_animating = state.any_window_open_animating();
        let fade_animating = state.any_surface_fading_in() || state.wallpaper_fading();
        let geometry_animating = state.any_window_geometry_animating();
        let snapshots_animating = state.snapshots.tick();
        let dock_autohide = state.update_dock_autohide();
        let dock_preview = state.sync_dock_preview();
        let dock_animating = dock_autohide || dock_preview || state.dock.is_animating();
        let launcher_animating = state.tick_launcher();
        let switcher_animating = state.tick_switcher();
        let power_dialog_animating = state.tick_power_dialog();
        let cursor_animating = state.is_cursor_animating();
        let toasts_animating = state.notifications.settle();
        let snap_animating = state.settle_snap_preview();
        let topbar_animating = state.topbar.is_animating();
        let flash_animating = state.screenshot_flash.tick(std::time::Instant::now());
        let osd_animating = state.session.osd.tick(std::time::Instant::now());
        let wheel_animating = state.tick_smooth_wheel();
        let shot_animating = state.tick_shot();
        let polkit_animating = state.tick_polkit();
        let keychain_animating = state.tick_keychain_prompt();
        let night_light_animating = state.tick_night_light_fade();
        let blank_animating = state.tick_blank_fade();

        if toasts_animating
            || night_light_animating
            || blank_animating
            || polkit_animating
            || keychain_animating
            || shot_animating
            || osd_animating
            || wheel_animating
            || snap_animating
            || topbar_animating
            || overview_animating
            || hover_animating
            || open_animating
            || fade_animating
            || geometry_animating
            || snapshots_animating
            || dock_animating
            || launcher_animating
            || switcher_animating
            || power_dialog_animating
            || cursor_animating
            || flash_animating
            || login_animating
        {
            state.queue_animation_redraw();
        }

        state.tick_idle();
        state.tick_session_end();
        state.tick_local_notifications();
        state.sync_session_hints();
        state.sync_shell_bus();
        let _ = state.display_handle.flush_clients();
    })?;

    tracing::info!("shutting down");
    if login_session {
        handoff::paint_console();
    }

    drop(ipc_server);

    Ok(())
}

fn init_session_services(
    event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState, config: &config::Config,
) -> anyhow::Result<()> {
    spawn_xwayland_satellite();
    update_session_managers_environment();
    spawn_autostart(&config.general.autostart);
    if config.general.xdg_autostart {
        xdg_autostart::spawn_all();
    }
    if config.general.polkit_agent {
        init_polkit_agent(event_loop, state);
    }
    init_topbar_clock(event_loop, state)?;
    init_system_status(event_loop);
    init_quick_settings_network(event_loop);
    init_notifications(event_loop, state);
    init_media(event_loop, state);
    init_app_menu(event_loop, state);
    init_dock_services(event_loop, state);
    init_logind(event_loop, state);
    init_screensaver(event_loop, state);
    init_shell_bus(event_loop, state);
    init_bluetooth_agent(event_loop);
    init_network_agent(event_loop);
    init_disk_space(event_loop);
    init_removable(event_loop);
    init_netbird(event_loop, state);
    init_headset(event_loop);
    init_keychain(event_loop, state);
    init_cast(event_loop, state);
    state.welcome_if_first_run();
    if !greeter::pam::available() {
        tracing::warn!("PAM is unavailable: the screen will not lock on idle, and a manual lock cannot be undone");
    }
    Ok(())
}

fn init_shell_bus(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<shell_bus::Event>("a shell bus request");
    match shell_bus::start(state.shell_snapshot(), send) {
        Ok(handle) => state.session.shell_bus = Some(handle),
        Err(err) => {
            tracing::warn!("{} is unavailable ({err}); apps fall back to reading glide.toml", glide_app::BUS_NAME);
            return;
        }
    }
    watch(event_loop, receiver, "the shell bus channel", GlideState::handle_shell_bus_event);
}

fn init_screensaver(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<screensaver::Event>("a screen saver request");
    match screensaver::start(send) {
        Ok(handle) => state.session.screensaver = Some(handle),
        Err(err) => {
            tracing::warn!(
                "{} is unavailable ({err}); browsers and video players cannot keep the screen on",
                screensaver::BUS_NAME
            );
            return;
        }
    }
    watch(event_loop, receiver, "the screen saver channel", GlideState::handle_screensaver_event);
}

fn init_bluetooth_agent(event_loop: &mut EventLoop<'static, GlideState>) {
    let (send, receiver) = sink::<bluetooth_agent::Event>("a Bluetooth pairing request");
    if let Err(err) = bluetooth_agent::start(send) {
        tracing::warn!("the Bluetooth pairing agent is unavailable ({err}); devices that need a code cannot pair");
        return;
    }
    watch(event_loop, receiver, "the Bluetooth agent channel", GlideState::handle_bluetooth_agent);
}

fn init_removable(event_loop: &mut EventLoop<'static, GlideState>) {
    let (send, receiver) = sink::<removable::Event>("a removable drive event");
    if let Err(err) = removable::start(send) {
        tracing::info!("removable drives are not announced ({err}); is udisks2 installed?");
        return;
    }
    watch(event_loop, receiver, "the removable drive channel", GlideState::handle_removable);
}

fn init_netbird(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    if !glide_netbird::installed() {
        return;
    }
    let (send, receiver) = sink::<netbird::Event>("a NetBird update");
    if watch(event_loop, receiver, "the NetBird channel", GlideState::handle_netbird) {
        state.start_netbird(send);
    }
}

fn init_headset(event_loop: &mut EventLoop<'static, GlideState>) {
    let (send, receiver) = sink::<headset::Event>("a headset event");
    if watch(event_loop, receiver, "the headset channel", GlideState::handle_headset) {
        let send = std::sync::Arc::new(send);
        headset::start(move |event| send(event));
    }
}

fn init_keychain(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<glide_ipc::keychain::Envelope<glide_ipc::keychain::Response>>("a keychain reply");
    if !watch(event_loop, receiver, "the keychain channel", GlideState::handle_keychain) {
        return;
    }
    match keychain::Keychain::start(send) {
        Ok(keychain) => {
            state.session.keychain = keychain;
            state.sync_passwords_applet();
        }
        Err(err) => tracing::warn!(%err, "the keychain link is unavailable; saved passwords cannot be filled"),
    }
}

fn init_cast(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<cast::Event>("a screen mirroring event");
    if watch(event_loop, receiver, "the screen mirroring channel", GlideState::handle_cast) {
        state.session.cast.attach(std::sync::Arc::new(send));
    }
}

fn init_disk_space(event_loop: &mut EventLoop<'static, GlideState>) {
    let (send, receiver) = sink::<disk_space::Low>("a low disk space warning");
    disk_space::start(send);
    watch(event_loop, receiver, "the disk space channel", GlideState::handle_low_disk);
}

fn init_polkit_agent(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (reports, report_channel) = sink::<polkit::Attempt>("a polkit helper report");
    if !watch(event_loop, report_channel, "the polkit helper channel", GlideState::handle_polkit_report) {
        return;
    }
    state.session.polkit.attach(reports);
    let (send, receiver) = sink::<polkit_agent::Event>("a polkit authentication request");
    if let Err(err) = polkit_agent::start(send) {
        tracing::warn!("the polkit agent is unavailable ({err}); privileged actions cannot ask for a password");
        return;
    }
    watch(event_loop, receiver, "the polkit agent channel", GlideState::handle_polkit);
}

fn init_network_agent(event_loop: &mut EventLoop<'static, GlideState>) {
    let (send, receiver) = sink::<nm_agent::Event>("a network secret request");
    if let Err(err) = nm_agent::start(send) {
        tracing::warn!("the network secret agent is unavailable ({err}); saved networks cannot ask for a password");
        return;
    }
    watch(event_loop, receiver, "the network agent channel", GlideState::handle_network_agent);
}

fn init_logind(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<logind::LoginEvent>("a logind lock request");
    let (handle, locked_at_start) = match logind::start(send) {
        Ok(started) => started,
        Err(err) => {
            tracing::warn!(?err, "failed to spawn the logind listener");
            return;
        }
    };
    state.session.logind = Some(handle);
    watch(event_loop, receiver, "the logind channel", |state, event| match event {
        logind::LoginEvent::Lock | logind::LoginEvent::Sleep => state.lock_screen(),
        logind::LoginEvent::Unlock => state.unlock_screen(),
        logind::LoginEvent::Wake => state.release_held_keys("woke from sleep"),
    });
    if locked_at_start.recv_timeout(logind::STARTUP_WAIT).unwrap_or(false) {
        state.lock_screen();
    }
}

fn init_topbar_clock(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) -> anyhow::Result<()> {
    use smithay::reexports::calloop::timer::{TimeoutAction, Timer};

    state.refresh_topbar();
    state.sync_topbar_layout();

    event_loop
        .handle()
        .insert_source(Timer::from_duration(std::time::Duration::from_secs(1)), |_, (), state| {
            reap_child_processes();
            let clock = state.refresh_topbar();
            let expired = state.notifications.tick();
            let night = state.tick_night_light();
            let appearance = state.tick_appearance_schedule();
            let power = state.tick_qs_power();
            let netbird = state.tick_netbird();
            state.tick_keychain_idle(std::time::Instant::now());
            if clock || expired || night || appearance || power || netbird {
                state.queue_redraw();
            }
            TimeoutAction::ToDuration(std::time::Duration::from_secs(1))
        })
        .map_err(|err| anyhow::anyhow!("failed to start the topbar clock: {err}"))?;
    Ok(())
}

fn init_greeter_timer(event_loop: &mut EventLoop<'static, GlideState>) -> anyhow::Result<()> {
    use smithay::reexports::calloop::timer::{TimeoutAction, Timer};

    event_loop
        .handle()
        .insert_source(Timer::from_duration(std::time::Duration::from_secs(1)), |_, (), state| {
            reap_child_processes();
            if state.tick_greeter() {
                state.queue_redraw();
            }
            TimeoutAction::ToDuration(std::time::Duration::from_secs(1))
        })
        .map_err(|err| anyhow::anyhow!("failed to start greeter timer: {err}"))?;
    Ok(())
}

fn sink<T: Send + 'static>(what: &'static str) -> (impl Fn(T) + Send + Sync + 'static, channel::Channel<T>) {
    let (sender, receiver) = channel::channel::<T>();
    let sender = std::sync::Mutex::new(sender);
    let send = move |value: T| {
        let Ok(sender) = sender.lock() else {
            return;
        };
        if let Err(err) = sender.send(value) {
            tracing::debug!("dropping {what}, the compositor is gone: {err}");
        }
    };
    (send, receiver)
}

fn watch<T: 'static>(
    event_loop: &mut EventLoop<'static, GlideState>, receiver: channel::Channel<T>, what: &'static str,
    mut handle: impl FnMut(&mut GlideState, T) + 'static,
) -> bool {
    let inserted = event_loop.handle().insert_source(receiver, move |event, (), state| {
        if let channel::Event::Msg(value) = event {
            handle(state, value);
        }
    });
    match inserted {
        Ok(_) => true,
        Err(err) => {
            tracing::error!("failed to watch {what}: {err}");
            false
        }
    }
}

fn init_system_status(event_loop: &mut EventLoop<'static, GlideState>) {
    let (send, receiver) = sink::<system_info::SystemStatus>("a system status sample");
    system_info::start_poller(send);
    if watch(event_loop, receiver, "the system status channel", GlideState::handle_system_status) {
        status_hooks::start();
    }
}

fn init_quick_settings_network(event_loop: &mut EventLoop<'static, GlideState>) {
    use smithay::reexports::calloop::timer::{TimeoutAction, Timer};

    let (send, receiver) = sink::<qs_net::NetEvent>("a network panel update");
    qs_net::install(send);
    watch(event_loop, receiver, "the network panel channel", GlideState::handle_net_event);

    let rescan = event_loop.handle().insert_source(Timer::from_duration(qs_net::RESCAN_INTERVAL), |_, (), state| {
        state.rescan_quick_settings_panel();
        TimeoutAction::ToDuration(qs_net::RESCAN_INTERVAL)
    });
    if let Err(err) = rescan {
        tracing::error!("failed to start the network panel rescan timer: {err}");
    }
}

fn init_notifications(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    state.notifications.load_senders(glide_config::senders::Senders::load());
    let (send, receiver) = sink::<glide_notify::NotifyEvent>("a notification");
    match glide_notify::start(std::sync::Arc::new(send)) {
        Ok(handle) => state.notifications.attach(handle),
        Err(err) => {
            tracing::warn!(
                "notifications are unavailable ({err}); another daemon may already own \
                 {}, or there is no session bus",
                glide_notify::BUS_NAME
            );
            return;
        }
    }
    watch(event_loop, receiver, "the notification channel", GlideState::handle_notify_event);
}

fn init_media(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<glide_media::MediaEvent>("a media update");
    match glide_media::start(std::sync::Arc::new(send)) {
        Ok(handle) => state.media = Some(handle),
        Err(err) => {
            tracing::warn!("media players are unavailable ({err}); is there a session bus?");
            return;
        }
    }
    watch(event_loop, receiver, "the media channel", GlideState::handle_media_event);

    let (send, receiver) = sink::<(String, media_art::DecodedArt)>("decoded cover art");
    let Some(worker) = media_art::start(move |url, art| send((url, art))) else {
        return;
    };
    if watch(event_loop, receiver, "the cover art channel", |state, (url, art)| state.handle_media_art(&url, art)) {
        state.media_art.attach(worker);
    }
}

fn init_dock_services(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<app_index::Reply>("a desktop entry lookup");
    let worker = app_index::start(send);
    if watch(event_loop, receiver, "the desktop entry index", GlideState::handle_app_index)
        && let Some(worker) = worker
    {
        state.app_directory.attach(worker);
    }

    let (send, receiver) = sink::<(String, launcher_entry::EntryUpdate)>("a launcher entry update");
    if let Err(err) = launcher_entry::start(move |desktop_id, update| send((desktop_id, update))) {
        tracing::warn!("dock badges are unavailable ({err}); is there a session bus?");
        return;
    }
    watch(event_loop, receiver, "the launcher entry channel", |state, (desktop_id, update)| {
        state.handle_launcher_entry(&desktop_id, &update);
    });
}

fn init_app_menu(event_loop: &mut EventLoop<'static, GlideState>, state: &mut GlideState) {
    let (send, receiver) = sink::<glide_appmenu::AppMenuEvent>("an app menu update");
    match glide_appmenu::start(std::sync::Arc::new(send), state.app_menus.registrar) {
        Ok(handle) => state.app_menus.handle = Some(handle),
        Err(err) => {
            tracing::warn!("the global menu is unavailable ({err}); is there a session bus?");
            return;
        }
    }
    watch(event_loop, receiver, "the app menu channel", GlideState::handle_app_menu_event);
}

const STALE_SESSION_VARS: &[&str] = &["DISPLAY", "XAUTHORITY", "GNOME_SETUP_DISPLAY", "WAYLAND_SOCKET"];

fn session_environment(socket_name: &std::ffi::OsStr) -> Vec<(&'static str, std::ffi::OsString)> {
    let mut vars: Vec<(&'static str, std::ffi::OsString)> = vec![
        ("WAYLAND_DISPLAY", socket_name.to_os_string()),
        ("XDG_SESSION_TYPE", "wayland".into()),
        ("XDG_CURRENT_DESKTOP", "Glide".into()),
        ("XDG_SESSION_DESKTOP", "glide".into()),
    ];
    let inherited = (std::env::var_os("XCURSOR_THEME").is_some(), std::env::var_os("XCURSOR_SIZE").is_some());
    vars.extend(cursor_environment(cursor::configured_theme(), cursor::configured_size(), inherited));
    vars
}

fn cursor_environment(
    theme: Option<String>, size: Option<u32>, (theme_inherited, size_inherited): (bool, bool),
) -> Vec<(&'static str, std::ffi::OsString)> {
    let theme = theme.or_else(|| (!theme_inherited).then(|| "default".to_string()));
    let size = size.map(|size| size.to_string()).or_else(|| (!size_inherited).then(|| "24".to_string()));
    let mut vars = Vec::new();
    if let Some(theme) = theme {
        vars.push(("XCURSOR_THEME", theme.into()));
    }
    if let Some(size) = size {
        vars.push(("XCURSOR_SIZE", size.into()));
    }
    vars
}

fn apply_session_environment(socket_name: &std::ffi::OsStr) {
    // SAFETY: called once on the main thread before the event loop runs. The threads started by
    // then (the GSettings worker, the wallpaper loader, Mesa's) are assumed to read the
    // environment only through std::env, which takes the same lock, or only during their startup.
    unsafe {
        for (key, value) in session_environment(socket_name) {
            std::env::set_var(key, value);
        }
        for key in STALE_SESSION_VARS {
            std::env::remove_var(key);
        }
    }
}

fn warn_about_a_foreign_session() {
    let Ok(desktop) = std::env::var("XDG_CURRENT_DESKTOP") else {
        return;
    };
    if desktop.is_empty() || desktop.eq_ignore_ascii_case("glide") {
        return;
    }
    if std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_none() {
        return;
    }
    tracing::warn!(
        foreign_desktop = %desktop,
        "sharing a D-Bus session bus with another desktop: single-instance apps \
         (kgx, nautilus, ...) already running there will open their windows there \
         instead of in Glide. Start Glide with `dbus-run-session -- glide` to \
         isolate it."
    );
}

const SESSION_ENV_VARS: &[&str] = &[
    "WAYLAND_DISPLAY",
    "XDG_SESSION_TYPE",
    "XDG_CURRENT_DESKTOP",
    "XDG_SESSION_DESKTOP",
    "DISPLAY",
    "GLIDE_IPC_SOCK",
    "XCURSOR_THEME",
    "XCURSOR_SIZE",
];

fn update_session_managers_environment() {
    let vars: Vec<String> =
        SESSION_ENV_VARS.iter().filter(|name| std::env::var_os(name).is_some()).map(ToString::to_string).collect();

    let deadline = std::time::Instant::now() + SESSION_ENV_BUDGET;
    let mut dbus = std::process::Command::new("dbus-update-activation-environment");
    dbus.arg("--systemd").args(&vars);
    let mut systemd = std::process::Command::new("systemctl");
    systemd.args(["--user", "import-environment"]).args(&vars);
    for (name, mut command) in [("dbus-update-activation-environment", dbus), ("systemctl import-environment", systemd)]
    {
        match command.spawn() {
            Ok(child) => {
                if let Some(child) = wait_until(child, deadline) {
                    tracing::warn!("{name} is still running; the session starts without waiting for it");
                    if let Ok(mut children) = DETACHED_CHILDREN.lock() {
                        children.push(child);
                    }
                }
            }
            Err(err) => tracing::info!("{name} unavailable: {err}"),
        }
    }
}

const SESSION_ENV_BUDGET: std::time::Duration = std::time::Duration::from_millis(1500);

fn wait_until(mut child: std::process::Child, deadline: std::time::Instant) -> Option<std::process::Child> {
    loop {
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Ok(None) => return Some(child),
        }
    }
}

fn spawn_xwayland_satellite() {
    if let Some(x11) = xwayland::start() {
        // SAFETY: called on the main thread during startup; the only other thread
        // touching processes now is the satellite supervisor, already blocked in
        // `wait()` on its first child, and it never reads the environment there.
        unsafe { std::env::set_var("DISPLAY", &x11) };
        tracing::info!(x11, "X11 apps land on this display");
    }
}

fn spawn_autostart(commands: &[String]) {
    for command in commands {
        match spawn_shell_command(command) {
            Ok(pid) => tracing::info!(command, pid, "spawned autostart command"),
            Err(err) => tracing::error!(command, "failed to spawn autostart command: {err}"),
        }
    }
}

static DETACHED_CHILDREN: std::sync::Mutex<Vec<std::process::Child>> = std::sync::Mutex::new(Vec::new());

pub fn spawn_detached(mut command: std::process::Command) -> std::io::Result<u32> {
    reap_child_processes();
    if let Some(terminal) = glide_config::terminal::configured() {
        command.env("TERMINAL", terminal);
    }
    if let Some(theme) = cursor::configured_theme() {
        command.env("XCURSOR_THEME", theme);
    }
    if let Some(size) = cursor::configured_size() {
        command.env("XCURSOR_SIZE", size.to_string());
    }
    if std::env::var_os("QT_QPA_PLATFORMTHEME").is_none()
        && let Some(theme) = theme_export::qt_platform_theme()
    {
        command.env("QT_QPA_PLATFORMTHEME", theme);
    }
    if let Some(style) = theme_export::qt_style_override() {
        command.env("QT_STYLE_OVERRIDE", style);
    }
    let child = command.spawn()?;
    let pid = child.id();
    if let Ok(mut children) = DETACHED_CHILDREN.lock() {
        children.push(child);
    }
    Ok(pid)
}

pub fn spawn_shell_command(command: &str) -> std::io::Result<u32> {
    spawn_shell_command_with_env(command, &[])
}

pub fn spawn_shell_command_with_env(command: &str, env: &[(&str, &str)]) -> std::io::Result<u32> {
    let mut shell = std::process::Command::new("sh");
    shell.arg("-c").arg(command);
    for (key, value) in env {
        shell.env(key, value);
    }
    spawn_detached(shell)
}

pub fn reap_child_processes() {
    if let Ok(mut children) = DETACHED_CHILDREN.lock() {
        children.retain_mut(|child| matches!(child.try_wait(), Ok(None)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_line_knows_help_version_and_greeter_and_rejects_the_rest() {
        let parse = |args: &[&str]| cli(args.iter().map(ToString::to_string));
        assert_eq!(parse(&[]), Cli::Run);
        assert_eq!(parse(&["--greeter"]), Cli::Run);
        assert_eq!(parse(&["--help"]), Cli::Help);
        assert_eq!(parse(&["--greeter", "-h"]), Cli::Help);
        assert_eq!(parse(&["-V"]), Cli::Version);
        assert_eq!(parse(&["--versoin"]), Cli::Unknown("--versoin".into()));
        assert!(USAGE.contains("--greeter") && USAGE.contains("--version"));
    }

    fn detached_child_count() -> usize {
        DETACHED_CHILDREN.lock().map_or(0, |children| children.len())
    }

    #[test]
    fn a_hung_session_env_helper_is_handed_back_at_the_deadline() {
        let started = std::time::Instant::now();
        let child = std::process::Command::new("sleep").arg("30").spawn().unwrap();
        let deadline = started + std::time::Duration::from_millis(100);
        let mut left = wait_until(child, deadline).expect("still running at the deadline");
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
        let _ = left.kill();
        let _ = left.wait();
    }

    #[test]
    fn a_finished_session_env_helper_is_not_kept() {
        let child = std::process::Command::new("true").spawn().unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        assert!(wait_until(child, deadline).is_none());
    }

    #[test]
    fn session_environment_points_clients_at_our_socket() {
        let vars = session_environment(std::ffi::OsStr::new("wayland-7"));
        let display = vars.iter().find(|(key, _)| *key == "WAYLAND_DISPLAY").expect("WAYLAND_DISPLAY must be set");
        assert_eq!(display.1, std::ffi::OsString::from("wayland-7"));
    }

    #[test]
    fn session_environment_identifies_the_desktop_as_glide() {
        let vars = session_environment(std::ffi::OsStr::new("wayland-0"));
        for (key, want) in
            [("XDG_CURRENT_DESKTOP", "Glide"), ("XDG_SESSION_DESKTOP", "glide"), ("XDG_SESSION_TYPE", "wayland")]
        {
            let got = vars.iter().find(|(k, _)| *k == key).unwrap_or_else(|| panic!("{key} must be set"));
            assert_eq!(got.1, std::ffi::OsString::from(want));
        }
    }

    #[test]
    fn the_configured_cursor_wins_over_the_inherited_one_at_startup() {
        let pairs = |vars: Vec<(&'static str, std::ffi::OsString)>| {
            vars.into_iter().map(|(key, value)| (key, value.to_string_lossy().into_owned())).collect::<Vec<_>>()
        };
        let configured = cursor_environment(Some("Bibata".into()), Some(32), (true, true));
        assert_eq!(pairs(configured), [("XCURSOR_THEME", "Bibata".to_string()), ("XCURSOR_SIZE", "32".to_string())]);
        assert!(cursor_environment(None, None, (true, true)).is_empty(), "an inherited cursor is left alone");
        assert_eq!(
            pairs(cursor_environment(None, None, (false, false))),
            [("XCURSOR_THEME", "default".to_string()), ("XCURSOR_SIZE", "24".to_string())]
        );
    }

    #[test]
    fn stale_session_vars_drop_a_foreign_x_server() {
        assert!(STALE_SESSION_VARS.contains(&"DISPLAY"));
        assert!(STALE_SESSION_VARS.contains(&"XAUTHORITY"));
    }

    #[test]
    fn session_environment_never_clears_what_it_sets() {
        let vars = session_environment(std::ffi::OsStr::new("wayland-0"));
        for (key, _) in &vars {
            assert!(!STALE_SESSION_VARS.contains(key), "{key} is both set and cleared");
        }
    }

    static CHILD_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn the_reaper_never_steals_a_child_another_thread_is_waiting_for() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let output = std::thread::spawn(|| std::process::Command::new("true").output());
        for _ in 0..200 {
            reap_child_processes();
        }
        let status =
            output.join().expect("thread must not panic").expect("output() must see its own exit status").status;
        assert!(status.success());
    }

    #[test]
    fn finished_detached_children_are_reaped() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        reap_child_processes();
        let before = detached_child_count();
        spawn_shell_command("true").expect("sh must exist");
        assert_eq!(detached_child_count(), before + 1);
        std::thread::sleep(std::time::Duration::from_millis(100));
        reap_child_processes();
        assert_eq!(detached_child_count(), before);
    }

    #[test]
    fn spawn_shell_command_runs_through_a_shell() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let marker = std::env::temp_dir().join(format!("glide-sh-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        spawn_shell_command(&format!("true; touch {}", marker.display())).expect("sh must exist");
        std::thread::sleep(std::time::Duration::from_millis(150));
        reap_child_processes();
        assert!(marker.exists(), "the shell did not run both commands");
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn a_launch_hands_the_activation_token_to_the_child() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let marker = std::env::temp_dir().join(format!("glide-env-test-{}", std::process::id()));
        let _ = std::fs::remove_file(&marker);
        spawn_shell_command_with_env(
            &format!("printf '%s %s' \"$XDG_ACTIVATION_TOKEN\" \"$DESKTOP_STARTUP_ID\" > {}", marker.display()),
            &[("XDG_ACTIVATION_TOKEN", "tok"), ("DESKTOP_STARTUP_ID", "tok")],
        )
        .expect("sh must exist");
        std::thread::sleep(std::time::Duration::from_millis(150));
        reap_child_processes();
        assert_eq!(std::fs::read_to_string(&marker).unwrap_or_default(), "tok tok");
        let _ = std::fs::remove_file(&marker);
    }

    #[test]
    fn spawn_autostart_does_not_panic_on_an_empty_list() {
        spawn_autostart(&[]);
    }

    #[test]
    fn spawn_autostart_runs_every_command_in_the_list() {
        let _guard = CHILD_TEST_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("glide-autostart-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let marker_a = dir.join("a");
        let marker_b = dir.join("b");

        spawn_autostart(&[format!("touch {}", marker_a.display()), format!("touch {}", marker_b.display())]);
        std::thread::sleep(std::time::Duration::from_millis(200));
        reap_child_processes();

        assert!(marker_a.exists(), "first autostart command did not run");
        assert!(marker_b.exists(), "second autostart command did not run");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
