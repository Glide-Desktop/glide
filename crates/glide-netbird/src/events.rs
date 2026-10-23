use std::time::SystemTime;

use crate::model::{Category, Event, Severity};
use crate::time::parse_rfc3339;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Silent,
    ProfileListChanged,
    ConfigChanged { by_policy: bool },
    SessionWarning { expires_at: Option<SystemTime>, final_warning: bool },
    SessionDeadlineRejected,
    UpdateAvailable { version: String, enforced: bool },
    Installing { version: Option<String> },
    Message { severity: Severity, category: Category, body: String },
}

impl Notice {
    #[must_use]
    pub const fn critical(&self) -> bool {
        matches!(self, Self::Message { severity: Severity::Critical, .. })
    }
}

#[must_use]
pub fn classify(event: &Event) -> Notice {
    match event.meta("kind") {
        Some("profile-list-changed") => return Notice::ProfileListChanged,
        Some("log-level-changed") => return Notice::Silent,
        _ => {}
    }
    match event.meta("type") {
        Some("config_changed") if event.category == Category::System => {
            return Notice::ConfigChanged { by_policy: event.meta("source") == Some("mdm") };
        }
        Some("policy_applied") => return Notice::Silent,
        _ => {}
    }
    if let Some(version) = event.meta("new_version_available") {
        return Notice::UpdateAvailable {
            version: version.to_string(),
            enforced: event.metadata.contains_key("enforced"),
        };
    }
    if event.meta("progress_window") == Some("show") {
        return Notice::Installing { version: event.meta("version").filter(|v| !v.is_empty()).map(str::to_string) };
    }
    if event.metadata.contains_key("progress_window") {
        return Notice::Silent;
    }
    if event.meta("session_deadline_rejected").is_some_and(|reason| !reason.is_empty()) {
        return Notice::SessionDeadlineRejected;
    }
    if event.meta("session_warning") == Some("true") {
        return Notice::SessionWarning {
            expires_at: event.meta("session_expires_at").and_then(parse_rfc3339),
            final_warning: event.meta("session_final_warning") == Some("true"),
        };
    }
    if event.category == Category::Network && event.meta("network") == Some("::/0") {
        return Notice::Silent;
    }
    if event.user_message.is_empty() {
        return Notice::Silent;
    }
    let body = match event.meta("id").filter(|id| !id.is_empty()) {
        Some(id) => format!("{} ID: {id}", event.user_message),
        None => event.user_message.clone(),
    };
    Notice::Message { severity: event.severity, category: event.category, body }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::time::{Duration, UNIX_EPOCH};

    use super::*;

    fn event(category: Category, user_message: &str, meta: &[(&str, &str)]) -> Event {
        Event {
            id: "e1".into(),
            category,
            user_message: user_message.into(),
            metadata: meta
                .iter()
                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                .collect::<BTreeMap<_, _>>(),
            ..Event::default()
        }
    }

    #[test]
    fn internal_markers_never_become_toasts() {
        for meta in [
            &[("kind", "log-level-changed"), ("level", "debug")][..],
            &[("type", "policy_applied")][..],
            &[("progress_window", "hide")][..],
        ] {
            assert_eq!(classify(&event(Category::System, "English text", meta)), Notice::Silent, "{meta:?}");
        }
        assert_eq!(
            classify(&event(Category::System, "", &[("kind", "profile-list-changed")])),
            Notice::ProfileListChanged
        );
        assert_eq!(classify(&event(Category::Network, "Exit node ::/0 added", &[("network", "::/0")])), Notice::Silent);
        assert_eq!(classify(&event(Category::Dns, "", &[])), Notice::Silent);
    }

    #[test]
    fn config_changes_are_refresh_signals_and_only_policy_ones_are_announced() {
        let startup = event(Category::System, "", &[("type", "config_changed"), ("source", "startup")]);
        assert_eq!(classify(&startup), Notice::ConfigChanged { by_policy: false });
        let policy = event(Category::System, "", &[("type", "config_changed"), ("source", "mdm")]);
        assert_eq!(classify(&policy), Notice::ConfigChanged { by_policy: true });
    }

    #[test]
    fn session_warnings_carry_their_deadline_and_whether_they_are_the_last_one() {
        let first = event(
            Category::Authentication,
            "",
            &[("session_warning", "true"), ("session_expires_at", "1970-01-01T00:10:00Z"), ("lead_minutes", "10")],
        );
        assert_eq!(
            classify(&first),
            Notice::SessionWarning { expires_at: Some(UNIX_EPOCH + Duration::from_mins(10)), final_warning: false }
        );
        let last =
            event(Category::Authentication, "", &[("session_warning", "true"), ("session_final_warning", "true")]);
        assert_eq!(classify(&last), Notice::SessionWarning { expires_at: None, final_warning: true });
        let rejected = event(Category::Authentication, "", &[("session_deadline_rejected", "too far ahead")]);
        assert_eq!(classify(&rejected), Notice::SessionDeadlineRejected);
    }

    #[test]
    fn update_announcements_and_install_progress_feed_the_update_state() {
        let manual = event(Category::System, "NetBird 0.81 is out", &[("new_version_available", "0.81.0")]);
        assert_eq!(classify(&manual), Notice::UpdateAvailable { version: "0.81.0".into(), enforced: false });
        let enforced = event(Category::System, "", &[("new_version_available", "0.81.0"), ("enforced", "")]);
        assert_eq!(classify(&enforced), Notice::UpdateAvailable { version: "0.81.0".into(), enforced: true });
        let installing = event(Category::System, "", &[("progress_window", "show"), ("version", "0.81.0")]);
        assert_eq!(classify(&installing), Notice::Installing { version: Some("0.81.0".into()) });
    }

    #[test]
    fn plain_events_with_a_user_message_become_messages_with_the_resource_id() {
        let dns = Event { severity: Severity::Critical, ..event(Category::Dns, "DNS is broken", &[("id", "r-7")]) };
        let notice = classify(&dns);
        assert!(notice.critical());
        assert_eq!(
            notice,
            Notice::Message {
                severity: Severity::Critical,
                category: Category::Dns,
                body: "DNS is broken ID: r-7".into()
            }
        );
    }
}
