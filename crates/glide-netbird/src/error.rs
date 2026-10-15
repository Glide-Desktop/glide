use tonic::Code;
use tonic_types::StatusExt;

const PRIVILEGE_REASON: &str = "PRIVILEGE_REQUIRED";
const PRIVILEGE_DOMAIN: &str = "daemon.netbird.io";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error("the NetBird service is not running")]
    Unavailable,
    #[error("{summary}")]
    PrivilegeRequired { summary: String, command: String },
    #[error("cancelled")]
    Cancelled,
    #[error("the NetBird service did not answer in time")]
    Timeout,
    #[error("{message}")]
    Daemon { code: Code, message: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    JwtClockSkew,
    JwtExpired,
    JwtSignatureInvalid,
    SessionExpired,
    InvalidSetupKey,
    PermissionDenied,
    PrivilegeRequired,
    DaemonUnreachable,
    Unimplemented,
    Unknown,
}

impl Error {
    #[must_use]
    pub fn from_status(status: &tonic::Status) -> Self {
        if let Some(info) = status.get_details_error_info()
            && info.reason == PRIVILEGE_REASON
            && info.domain == PRIVILEGE_DOMAIN
        {
            let summary = info.metadata.get("summary").filter(|text| !text.is_empty()).cloned();
            return Self::PrivilegeRequired {
                summary: summary.unwrap_or_else(|| status.message().to_string()),
                command: info.metadata.get("command").cloned().unwrap_or_default(),
            };
        }
        match status.code() {
            Code::Unavailable => Self::Unavailable,
            Code::Cancelled => Self::Cancelled,
            Code::DeadlineExceeded => Self::Timeout,
            code => Self::Daemon { code, message: status.message().to_string() },
        }
    }

    #[must_use]
    pub fn reason(&self) -> Reason {
        let (code, message) = match self {
            Self::Unavailable | Self::Timeout => return Reason::DaemonUnreachable,
            Self::PrivilegeRequired { .. } => return Reason::PrivilegeRequired,
            Self::Cancelled => return Reason::Unknown,
            Self::Daemon { code, message } => (*code, message.to_lowercase()),
        };
        let has = |needle: &str| message.contains(needle);
        if has("token used before issued") || has("token is not valid yet") {
            Reason::JwtClockSkew
        } else if has("token is expired") || has("token has expired") {
            Reason::JwtExpired
        } else if has("token signature is invalid") {
            Reason::JwtSignatureInvalid
        } else if has("peer login has expired") {
            Reason::SessionExpired
        } else if has("invalid setup-key") || has("invalid setup key") {
            Reason::InvalidSetupKey
        } else if has("permission denied") || code == Code::PermissionDenied {
            Reason::PermissionDenied
        } else if has("connection refused") || has("context deadline exceeded") {
            Reason::DaemonUnreachable
        } else if code == Code::Unimplemented {
            Reason::Unimplemented
        } else {
            Reason::Unknown
        }
    }

    #[must_use]
    pub const fn is_unavailable(&self) -> bool {
        matches!(self, Self::Unavailable)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn daemon(code: Code, message: &str) -> Error {
        Error::from_status(&tonic::Status::new(code, message))
    }

    #[test]
    fn transport_failures_mean_the_service_is_not_running() {
        assert_eq!(daemon(Code::Unavailable, "transport error"), Error::Unavailable);
        assert_eq!(daemon(Code::DeadlineExceeded, "late").reason(), Reason::DaemonUnreachable);
        assert_eq!(daemon(Code::Cancelled, "superseded"), Error::Cancelled);
    }

    #[test]
    fn daemon_messages_are_sorted_into_the_reasons_netbird_ui_names() {
        let cases = [
            ("rpc error: token used before issued", Reason::JwtClockSkew),
            ("Token is expired", Reason::JwtExpired),
            ("token signature is invalid", Reason::JwtSignatureInvalid),
            ("peer login has expired, please log in once more", Reason::SessionExpired),
            ("invalid setup-key or no usage left", Reason::InvalidSetupKey),
            ("permission denied", Reason::PermissionDenied),
            ("something else", Reason::Unknown),
        ];
        for (message, reason) in cases {
            assert_eq!(daemon(Code::Unknown, message).reason(), reason, "{message}");
        }
        assert_eq!(daemon(Code::PermissionDenied, "nope").reason(), Reason::PermissionDenied);
        assert_eq!(daemon(Code::Unimplemented, "unknown method").reason(), Reason::Unimplemented);
    }

    #[test]
    fn a_privilege_refusal_carries_the_summary_and_the_command_to_run() {
        let info = tonic_types::ErrorDetails::with_error_info(
            PRIVILEGE_REASON,
            PRIVILEGE_DOMAIN,
            [
                ("summary".to_string(), "Enabling the SSH server needs root".to_string()),
                ("command".to_string(), "sudo netbird up --allow-server-ssh".to_string()),
            ],
        );
        let status = tonic::Status::with_error_details(Code::PermissionDenied, "refused", info);
        assert_eq!(
            Error::from_status(&status),
            Error::PrivilegeRequired {
                summary: "Enabling the SSH server needs root".into(),
                command: "sudo netbird up --allow-server-ssh".into(),
            }
        );
    }
}
