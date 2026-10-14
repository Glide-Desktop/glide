use std::path::Path;
use std::process::{Command, Stdio};

use super::run;

pub const CUPS_URL: &str = "http://localhost:631";
pub const TEST_PAGE: &str = "/usr/share/cups/data/testprint";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrinterState {
    Idle,
    Printing,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Printer {
    pub name: String,
    pub state: PrinterState,
    pub job: Option<String>,
    pub reason: Option<String>,
    pub device: Option<String>,
    pub is_default: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub printer: String,
    pub user: String,
    pub size: u64,
    pub submitted: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrintState {
    pub available: bool,
    pub running: bool,
    pub printers: Vec<Printer>,
    pub jobs: Vec<Job>,
    pub test_page: bool,
}

#[must_use]
pub fn parse_printers(text: &str) -> (Vec<Printer>, Option<String>) {
    let mut printers: Vec<Printer> = Vec::new();
    let mut default = None;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("printer ") {
            let Some((name, status)) = rest.split_once(' ') else {
                continue;
            };
            let status = status.trim();
            let status = status.strip_prefix("is ").unwrap_or(status);
            let (state, job) = if let Some(rest) = status.strip_prefix("now printing ") {
                let job = rest.split_whitespace().next().map(|job| job.trim_end_matches('.').to_string());
                (PrinterState::Printing, job)
            } else if status.starts_with("disabled") {
                (PrinterState::Disabled, None)
            } else {
                (PrinterState::Idle, None)
            };
            printers.push(Printer {
                name: name.to_string(),
                state,
                job,
                reason: None,
                device: None,
                is_default: false,
            });
        } else if let Some(name) = line.strip_prefix("system default destination:") {
            let name = name.trim();
            if !name.is_empty() {
                default = Some(name.to_string());
            }
        } else if line.starts_with(char::is_whitespace)
            && let Some(last) = printers.last_mut()
        {
            let reason = line.trim();
            if !reason.is_empty() && reason != "-" && last.reason.is_none() {
                last.reason = Some(reason.to_string());
            }
        }
    }
    for printer in &mut printers {
        printer.is_default = default.as_deref() == Some(printer.name.as_str());
    }
    (printers, default)
}

#[must_use]
pub fn parse_devices(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let rest = line.strip_prefix("device for ")?;
            let (name, uri) = rest.split_once(':')?;
            Some((name.trim().to_string(), uri.trim().to_string()))
        })
        .collect()
}

#[must_use]
pub fn parse_jobs(text: &str, printers: &[Printer]) -> Vec<Job> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            let id = parts.next()?.to_string();
            let user = parts.next()?.to_string();
            let size = parts.next()?.parse().ok()?;
            let submitted = parts.collect::<Vec<_>>().join(" ");
            let printer = printers
                .iter()
                .map(|printer| printer.name.as_str())
                .filter(|name| {
                    id.strip_prefix(name)
                        .and_then(|rest| rest.strip_prefix('-'))
                        .is_some_and(|number| number.chars().all(|ch| ch.is_ascii_digit()))
                })
                .max_by_key(|name| name.len())
                .map_or_else(
                    || id.rsplit_once('-').map_or_else(|| id.clone(), |(name, _)| name.to_string()),
                    str::to_string,
                );
            Some(Job { id, printer, user, size, submitted })
        })
        .collect()
}

#[must_use]
pub fn connection_label(uri: &str) -> &'static str {
    let scheme = uri.split(':').next().unwrap_or_default();
    match scheme {
        "usb" => "USB",
        "ipp" | "ipps" | "http" | "https" | "socket" | "lpd" | "dnssd" | "smb" | "implicitclass" => "network",
        "file" | "cups-pdf" => "file",
        _ => "other",
    }
}

#[must_use]
pub fn size_label(bytes: u64) -> String {
    let value = bytes as f64;
    if bytes < 1024 {
        format!("{bytes} B")
    } else if value < 1024.0 * 1024.0 {
        format!("{:.0} KB", value / 1024.0)
    } else {
        format!("{:.1} MB", value / 1024.0 / 1024.0)
    }
}

fn c_locale(program: &str, args: &[&str]) -> Result<String, String> {
    let result = Command::new(program)
        .args(args)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()
        .map_err(|err| format!("{program}: {err}"))?;
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    if result.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&result.stderr).trim().to_string();
        Err(if stderr.is_empty() { stdout.trim().to_string() } else { stderr })
    }
}

#[must_use]
pub fn read() -> PrintState {
    if !run::available("lpstat") {
        return PrintState::default();
    }
    let running = c_locale("lpstat", &["-r"]).is_ok_and(|text| text.contains("is running"));
    let (mut printers, _) = c_locale("lpstat", &["-p", "-d"]).map(|text| parse_printers(&text)).unwrap_or_default();
    if let Ok(text) = c_locale("lpstat", &["-v"]) {
        for (name, uri) in parse_devices(&text) {
            if let Some(printer) = printers.iter_mut().find(|printer| printer.name == name) {
                printer.device = Some(uri);
            }
        }
    }
    let jobs = c_locale("lpstat", &["-o"]).map(|text| parse_jobs(&text, &printers)).unwrap_or_default();
    PrintState {
        available: true,
        running,
        printers,
        jobs,
        test_page: Path::new(TEST_PAGE).is_file() && run::available("lp"),
    }
}

pub fn set_default(name: &str) -> Result<String, String> {
    c_locale("lpoptions", &["-d", name])
}

pub fn cancel(job: &str) -> Result<String, String> {
    c_locale("cancel", &[job])
}

pub fn set_enabled(name: &str, enabled: bool) -> Result<String, String> {
    c_locale(if enabled { "cupsenable" } else { "cupsdisable" }, &[name])
}

pub fn print_test_page(name: &str) -> Result<String, String> {
    c_locale("lp", &["-d", name, TEST_PAGE])
}

pub fn open_web_interface() -> Result<String, String> {
    Command::new("xdg-open")
        .arg(CUPS_URL)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .and_then(|mut child| child.wait())
        .map_err(|err| format!("xdg-open: {err}"))
        .and_then(
            |status| {
                if status.success() { Ok(String::new()) } else { Err(format!("xdg-open {CUPS_URL}: {status}")) }
            },
        )
}
