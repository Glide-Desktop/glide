use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SYSTEM_BINARIES: [&str; 2] = ["/usr/bin/facekey", "/usr/local/bin/facekey"];
pub const ATTEMPTS_FILE: &str = "facekey-attempts.jsonl";
pub const KEPT_DAYS: i64 = 30;
pub const PAM_SERVICES: [PamService; 3] = [PamService::Polkit, PamService::Sudo, PamService::Login];

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    pub lock_screen: bool,
    pub camera: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self { lock_screen: true, camera: None }
    }
}

#[must_use]
pub fn binary() -> Option<PathBuf> {
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("facekey")))
        .filter(|path| path.is_absolute() && path.is_file());
    sibling.or_else(|| SYSTEM_BINARIES.iter().map(PathBuf::from).find(|path| path.is_file()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    Lock,
    Polkit,
    Sudo,
    Login,
    Test,
}

impl Source {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "lock" => Some(Self::Lock),
            "polkit" => Some(Self::Polkit),
            "sudo" => Some(Self::Sudo),
            "login" => Some(Self::Login),
            "test" => Some(Self::Test),
            _ => None,
        }
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Lock => "lock",
            Self::Polkit => "polkit",
            Self::Sudo => "sudo",
            Self::Login => "login",
            Self::Test => "test",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PamService {
    Polkit,
    Sudo,
    Login,
}

impl PamService {
    #[must_use]
    pub const fn file(self) -> &'static str {
        match self {
            Self::Polkit => "polkit-1",
            Self::Sudo => "sudo",
            Self::Login => "greetd",
        }
    }

    #[must_use]
    pub const fn source(self) -> Source {
        match self {
            Self::Polkit => Source::Polkit,
            Self::Sudo => Source::Sudo,
            Self::Login => Source::Login,
        }
    }

    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        PAM_SERVICES.into_iter().find(|service| service.file() == text || service.source().name() == text)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Reason {
    Passed,
    NoFace,
    TooDark,
    TooBright,
    Blurry,
    TooFar,
    OffFrame,
    HeadTurned,
    Moving,
    Spoof,
    Mismatch,
    OtherPerson,
    Timeout,
    NoCamera,
    CameraBusy,
    CameraUntrusted,
    NotEnrolled,
    StaleProfile,
    Remote,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Camera {
    pub path: String,
    pub name: String,
    pub trusted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refusal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Event {
    Started { camera: String },
    Frame { width: u32, height: u32, jpeg: String },
    Enroll { step: u8, hold: u8, required: u8, face: bool, hint: Option<Reason> },
    Verify { consensus: u8, required: u8, face: bool },
    Finished { success: bool, reason: Reason },
    Failed { reason: Reason, message: String },
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Status {
    pub enrolled: bool,
    pub user: String,
    pub compatible: Option<bool>,
    pub model_name: Option<String>,
    pub template_hash: Option<String>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub num_samples: Option<i32>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt {
    pub at: i64,
    pub source: Source,
    pub success: bool,
    pub reason: Reason,
}

#[must_use]
pub fn user_attempts_path() -> Option<PathBuf> {
    crate::state_home().map(|dir| dir.join("glide").join(ATTEMPTS_FILE))
}

#[must_use]
pub fn system_attempts_path(user: &str) -> PathBuf {
    Path::new("/var/lib/glide-face").join(user).join("attempts.jsonl")
}

#[must_use]
pub fn parse_attempts(text: &str, now: i64) -> Vec<Attempt> {
    let oldest = now - KEPT_DAYS * 86_400;
    text.lines()
        .filter_map(|line| serde_json::from_str::<Attempt>(line.trim()).ok())
        .filter(|attempt| attempt.at >= oldest && attempt.source != Source::Test)
        .collect()
}

#[must_use]
pub fn attempt_line(attempt: &Attempt) -> String {
    serde_json::to_string(attempt).unwrap_or_default()
}

#[must_use]
pub fn recent_attempts(texts: &[String], now: i64, limit: usize) -> Vec<Attempt> {
    let mut all: Vec<Attempt> = texts.iter().flat_map(|text| parse_attempts(text, now)).collect();
    all.sort_by_key(|attempt| std::cmp::Reverse(attempt.at));
    all.truncate(limit);
    all
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_lines_round_trip_and_old_or_test_attempts_are_dropped() {
        let now = 100 * 86_400;
        let kept = Attempt { at: now - 60, source: Source::Lock, success: false, reason: Reason::TooDark };
        let old = Attempt { at: now - (KEPT_DAYS + 1) * 86_400, ..kept };
        let trial = Attempt { source: Source::Test, ..kept };
        let lines = [kept, old, trial].iter().map(attempt_line).collect::<Vec<_>>().join("\n");
        assert_eq!(parse_attempts(&format!("{lines}\nnot json\n"), now), vec![kept]);
        assert!(attempt_line(&kept).contains("\"reason\":\"too-dark\""));
    }

    #[test]
    fn recent_attempts_merge_both_logs_newest_first() {
        let a = Attempt { at: 100, source: Source::Lock, success: true, reason: Reason::Passed };
        let b = Attempt { at: 200, source: Source::Sudo, ..a };
        let merged = recent_attempts(&[attempt_line(&a), attempt_line(&b)], 300, 1);
        assert_eq!(merged, vec![b]);
    }

    #[test]
    fn pam_services_parse_from_file_or_source_names() {
        assert_eq!(PamService::parse("polkit-1"), Some(PamService::Polkit));
        assert_eq!(PamService::parse("login"), Some(PamService::Login));
        assert_eq!(PamService::parse("/etc/pam.d/sudo"), None);
    }
}
