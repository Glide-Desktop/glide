use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use glide_netbird::{self as nb, ConfigPatch};

pub const APPLY_FLAG: &str = "--netbird-apply";
const STEP: Duration = Duration::from_millis(250);

fn daemon() -> Result<nb::Daemon, nb::Error> {
    static DAEMON: OnceLock<Result<nb::Daemon, nb::Error>> = OnceLock::new();
    DAEMON.get_or_init(nb::Daemon::connect).clone()
}

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub status: nb::Status,
    pub active: nb::ActiveProfile,
    pub config: Option<nb::Config>,
    pub features: nb::Features,
    pub profiles: Vec<nb::Profile>,
    pub networks: Vec<nb::Network>,
    pub forwarding: Vec<nb::ForwardingRule>,
    pub compatible: bool,
}

#[must_use]
pub fn snapshot(full: bool) -> Snapshot {
    let Ok(daemon) = daemon() else {
        return Snapshot { status: nb::Status::unavailable(), ..Snapshot::default() };
    };
    let Ok(status) = daemon.status() else {
        return Snapshot { status: nb::Status::unavailable(), ..Snapshot::default() };
    };
    let connected = status.state.is_connected();
    Snapshot {
        active: daemon.active_profile().unwrap_or_default(),
        config: daemon.config().ok(),
        features: daemon.features().unwrap_or_default(),
        profiles: daemon.profiles().unwrap_or_default(),
        networks: if connected { daemon.networks().unwrap_or_default() } else { Vec::new() },
        forwarding: if full && connected { daemon.forwarding_rules().unwrap_or_default() } else { Vec::new() },
        compatible: !full || daemon.compatible().unwrap_or(true),
        status,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    Daemon(nb::Error),
    Declined,
    Privilege { summary: String, command: String },
}

impl From<nb::Error> for Failure {
    fn from(error: nb::Error) -> Self {
        match error {
            nb::Error::PrivilegeRequired { summary, command } => Self::Privilege { summary, command },
            other => Self::Daemon(other),
        }
    }
}

pub(crate) fn open_uri(uri: &str) {
    let browser = std::env::var("BROWSER").ok().and_then(|list| {
        let first = list.split(':').map(str::trim).find(|entry| !entry.is_empty())?.to_string();
        let mut words = first.split_whitespace().map(str::to_string);
        Some((words.next()?, words.collect::<Vec<_>>()))
    });
    let spawned = browser
        .and_then(|(program, args)| Command::new(program).args(args).arg(uri).spawn().ok())
        .map_or_else(|| Command::new("xdg-open").arg(uri).spawn(), Ok);
    match spawned {
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(err) => eprintln!("glide-settings: could not open a browser: {err}"),
    }
}

pub fn open_folder(path: &str) {
    let folder = Path::new(path).parent().unwrap_or_else(|| Path::new(path));
    open_uri(&folder.to_string_lossy());
}

pub fn up() -> Result<(), Failure> {
    let daemon = daemon()?;
    if daemon.status()?.state.needs_login() {
        return sign_in();
    }
    Ok(daemon.up()?)
}

pub fn down() -> Result<(), Failure> {
    Ok(daemon()?.down()?)
}

pub fn sign_in() -> Result<(), Failure> {
    let daemon = daemon()?;
    let start = daemon.login()?;
    if start.needs_sso
        && let Some(uri) = nb::web_uri(&start.uri)
    {
        open_uri(uri);
    }
    Ok(daemon.finish_login(&start)?)
}

pub fn extend() -> Result<(), Failure> {
    let daemon = daemon()?;
    let start = daemon.request_extend()?;
    if let Some(uri) = nb::web_uri(&start.uri) {
        open_uri(uri);
    }
    daemon.wait_extend(&start)?;
    Ok(())
}

pub fn switch_profile(id: &str) -> Result<(), Failure> {
    daemon()?.switch_profile(id, true)?;
    Ok(())
}

pub fn add_profile(name: &str, management_url: Option<&str>) -> Result<(), Failure> {
    let daemon = daemon()?;
    let id = daemon.add_profile(name)?;
    if let Some(url) = management_url.filter(|url| !url.trim().is_empty()) {
        daemon.configure_profile(&id, url.trim())?;
    }
    Ok(())
}

pub fn rename_profile(id: &str, name: &str) -> Result<(), Failure> {
    Ok(daemon()?.rename_profile(id, name)?)
}

pub fn remove_profile(id: &str) -> Result<(), Failure> {
    Ok(daemon()?.remove_profile(id)?)
}

pub fn deregister(id: &str) -> Result<(), Failure> {
    Ok(daemon()?.deregister(id)?)
}

pub fn select(ids: Vec<String>, on: bool) -> Result<(), Failure> {
    let daemon = daemon()?;
    let result = if on { daemon.select_networks(ids, true) } else { daemon.deselect_networks(ids, true) };
    Ok(result?)
}

pub fn select_all(on: bool) -> Result<(), Failure> {
    let daemon = daemon()?;
    let result = if on { daemon.select_all_networks() } else { daemon.deselect_all_networks() };
    Ok(result?)
}

pub fn trigger_update() -> Result<(), Failure> {
    let result = daemon()?.trigger_update()?;
    if result.success {
        Ok(())
    } else {
        Err(Failure::Daemon(nb::Error::Daemon { code: nb::Code::Unknown, message: result.error }))
    }
}

fn elevate(
    profile: &nb::ActiveProfile, guarded: &ConfigPatch, summary: String, command: String,
) -> Result<(), Failure> {
    let Ok(exe) = std::env::current_exe() else {
        return Err(Failure::Privilege { summary, command });
    };
    let mut args = vec![
        exe.to_string_lossy().into_owned(),
        APPLY_FLAG.to_string(),
        "--user".to_string(),
        profile.username.clone(),
        "--profile".to_string(),
        profile.id.clone(),
    ];
    if let Some(url) = &guarded.management_url {
        args.extend(["--management-url".to_string(), url.clone()]);
    }
    for (flag, value) in [
        ("--allow-server-ssh", guarded.server_ssh_allowed),
        ("--enable-ssh-root", guarded.enable_ssh_root),
        ("--disable-ssh-auth", guarded.disable_ssh_auth),
    ] {
        if let Some(value) = value {
            args.push(format!("{flag}={value}"));
        }
    }
    match Command::new("pkexec").args(&args).status() {
        Ok(status) if status.success() => Ok(()),
        Ok(status) if matches!(status.code(), Some(126 | 127)) => Err(Failure::Declined),
        Ok(_) | Err(_) => Err(Failure::Privilege { summary, command }),
    }
}

pub fn set_config(patch: ConfigPatch) -> Result<(), Failure> {
    let daemon = daemon()?;
    match daemon.set_config(patch.clone()) {
        Ok(()) => Ok(()),
        Err(nb::Error::PrivilegeRequired { summary, command }) => {
            let guarded = patch.guarded();
            if guarded.is_empty() {
                return Err(Failure::Privilege { summary, command });
            }
            elevate(&daemon.active_profile()?, &guarded, summary, command)?;
            Ok(daemon.set_config(patch)?)
        }
        Err(err) => Err(err.into()),
    }
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct ApplyRequest {
    pub profile: nb::ActiveProfile,
    pub patch: ConfigPatch,
}

pub(crate) fn parse_apply(args: &[String]) -> Result<ApplyRequest, String> {
    let mut request = ApplyRequest::default();
    let mut words = args.iter();
    while let Some(word) = words.next() {
        let (flag, inline) = word.split_once('=').map_or((word.as_str(), None), |(flag, value)| (flag, Some(value)));
        let mut value =
            || inline.map(str::to_string).or_else(|| words.next().cloned()).ok_or(format!("{flag} needs a value"));
        let flag_bool = |text: String| parse_bool(&text).ok_or(format!("{flag}: expected true or false"));
        match flag {
            "--user" => request.profile.username = value()?,
            "--profile" => request.profile.id = value()?,
            "--management-url" => request.patch.management_url = Some(value()?),
            "--allow-server-ssh" => request.patch.server_ssh_allowed = Some(flag_bool(value()?)?),
            "--enable-ssh-root" => request.patch.enable_ssh_root = Some(flag_bool(value()?)?),
            "--disable-ssh-auth" => request.patch.disable_ssh_auth = Some(flag_bool(value()?)?),
            other => return Err(format!("unknown option {other}")),
        }
    }
    if request.profile.username.is_empty() || request.profile.id.is_empty() {
        return Err("--user and --profile are required".to_string());
    }
    if request.patch.is_empty() {
        return Err("nothing to change".to_string());
    }
    Ok(request)
}

#[must_use]
pub fn apply_privileged(args: &[String]) -> i32 {
    let request = match parse_apply(args) {
        Ok(request) => request,
        Err(err) => {
            eprintln!("glide-settings {APPLY_FLAG}: {err}");
            return 2;
        }
    };
    match daemon().and_then(|daemon| daemon.set_config_for(&request.profile, request.patch)) {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("glide-settings {APPLY_FLAG}: {err}");
            1
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BundleJob {
    pub options: nb::BundleOptions,
    pub trace: bool,
    pub packets: bool,
    pub capture: Option<Duration>,
}

fn wait(limit: Duration, cancel: &AtomicBool) -> bool {
    let started = Instant::now();
    while started.elapsed() < limit {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(STEP);
    }
    true
}

pub(crate) fn create_bundle(job: BundleJob, cancel: &AtomicBool) -> Result<nb::BundleResult, Failure> {
    let daemon = daemon()?;
    let previous = if job.trace { daemon.log_level().ok() } else { None };
    let captured: Result<bool, Failure> = 'capture: {
        if job.trace
            && let Err(err) = daemon.set_log_level(nb::LogLevel::Trace)
        {
            break 'capture Err(err.into());
        }
        if job.packets
            && let Err(err) = daemon.start_bundle_capture(job.capture.unwrap_or(Duration::from_mins(1)))
        {
            break 'capture Err(err.into());
        }
        let finished = job.capture.is_none_or(|limit| wait(limit, cancel));
        if job.packets
            && let Err(err) = daemon.stop_bundle_capture()
        {
            eprintln!("glide-settings: the packet capture did not stop cleanly: {err}");
        }
        Ok(finished)
    };
    if let Some(level) = previous
        && let Err(err) = daemon.set_log_level(level)
    {
        eprintln!("glide-settings: the NetBird log level was not restored: {err}");
    }
    if !captured? {
        return Err(Failure::Daemon(nb::Error::Cancelled));
    }
    Ok(daemon.debug_bundle(job.options)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|word| (*word).to_string()).collect()
    }

    #[test]
    fn the_elevated_mode_reads_the_guarded_settings_and_names_its_profile() {
        let request = parse_apply(&args(&[
            "--user",
            "artem",
            "--profile=work",
            "--allow-server-ssh=true",
            "--disable-ssh-auth",
            "false",
            "--management-url",
            "https://nb.example:443",
        ]))
        .expect("valid request");
        assert_eq!(request.profile.username, "artem");
        assert_eq!(request.profile.id, "work");
        assert_eq!(request.patch.server_ssh_allowed, Some(true));
        assert_eq!(request.patch.disable_ssh_auth, Some(false));
        assert_eq!(request.patch.management_url.as_deref(), Some("https://nb.example:443"));
        assert_eq!(request.patch.enable_ssh_root, None);
    }

    #[test]
    fn the_elevated_mode_refuses_anything_it_was_not_built_for() {
        for words in [
            &["--user", "a", "--profile", "p"][..],
            &["--profile", "p", "--allow-server-ssh=true"][..],
            &["--user", "a", "--profile", "p", "--disable-dns=true"][..],
            &["--user", "a", "--profile", "p", "--enable-ssh-root=maybe"][..],
            &["--user"][..],
        ] {
            assert!(parse_apply(&args(words)).is_err(), "{words:?}");
        }
    }

    #[test]
    fn a_cancelled_capture_stops_waiting_at_once() {
        let cancel = AtomicBool::new(true);
        let started = Instant::now();
        assert!(!wait(Duration::from_secs(30), &cancel));
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(wait(Duration::ZERO, &AtomicBool::new(false)));
    }

    #[test]
    fn a_privilege_refusal_keeps_the_command_to_run_by_hand() {
        let failure = Failure::from(nb::Error::PrivilegeRequired {
            summary: "Needs root".into(),
            command: "sudo netbird up --allow-server-ssh".into(),
        });
        assert_eq!(
            failure,
            Failure::Privilege { summary: "Needs root".into(), command: "sudo netbird up --allow-server-ssh".into() }
        );
        assert_eq!(Failure::from(nb::Error::Unavailable), Failure::Daemon(nb::Error::Unavailable));
    }
}
