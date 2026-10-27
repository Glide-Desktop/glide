use std::time::Duration;

pub const DEFAULT_TIMEOUT_LOW: Duration = Duration::from_secs(5);
pub const DEFAULT_TIMEOUT_NORMAL: Duration = Duration::from_secs(8);

pub const ACTION_DEFAULT: &str = "default";
pub const ACTION_SETTINGS: &str = "settings";
pub const ACTION_INLINE_REPLY: &str = "inline-reply";

pub const MAX_BODY_CHARS: usize = 600;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Urgency {
    Low,
    #[default]
    Normal,
    Critical,
}

impl Urgency {
    #[must_use]
    pub const fn from_hint(value: u8) -> Self {
        match value {
            0 => Self::Low,
            2 => Self::Critical,
            _ => Self::Normal,
        }
    }

    #[must_use]
    pub const fn default_timeout(self) -> Option<Duration> {
        match self {
            Self::Low => Some(DEFAULT_TIMEOUT_LOW),
            Self::Normal => Some(DEFAULT_TIMEOUT_NORMAL),
            Self::Critical => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CloseReason {
    Expired = 1,
    Dismissed = 2,
    Revoked = 3,
    Undefined = 4,
}

impl CloseReason {
    #[must_use]
    pub const fn as_wire(self) -> u32 {
        self as u32
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub key: String,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reply {
    pub key: String,
    pub label: String,
    pub placeholder: String,
    pub secret: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Sound {
    #[default]
    None,
    Named(String),
    File(String),
}

// Independent flags of the freedesktop notification wire format.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Notification {
    pub id: u32,
    pub sender: String,
    pub app_name: String,
    pub desktop_entry: String,
    pub app_icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<Action>,
    pub has_default_action: bool,
    pub has_settings_action: bool,
    pub reply: Option<Reply>,
    pub urgency: Urgency,
    pub category: String,
    pub resident: bool,
    pub transient: bool,
    pub timeout: Option<Duration>,
    pub sound: Sound,
}

impl Notification {
    #[must_use]
    pub fn group_key(&self) -> String {
        first_filled(&[&self.desktop_entry, &self.app_name]).unwrap_or(self.sender.trim()).to_lowercase()
    }

    #[must_use]
    pub fn display_app(&self) -> &str {
        first_filled(&[&self.app_name, &self.desktop_entry]).unwrap_or("Notification")
    }

    #[must_use]
    pub fn is_duplicate_of(&self, other: &Self) -> bool {
        self.summary == other.summary
            && self.body == other.body
            && self.app_name == other.app_name
            && self.desktop_entry == other.desktop_entry
            && self.actions == other.actions
    }
}

fn first_filled<'a>(fields: &[&'a String]) -> Option<&'a str> {
    fields.iter().map(|field| field.trim()).find(|field| !field.is_empty())
}

#[derive(Debug, Clone, PartialEq)]
pub enum NotifyEvent {
    Added(Box<Notification>),
    Replaced(Box<Notification>),
    Closed(u32, CloseReason),
    InhibitedChanged(bool),
}

#[must_use]
pub fn resolve_timeout(expire_timeout: i32, urgency: Urgency) -> Option<Duration> {
    if urgency == Urgency::Critical {
        return None;
    }
    match expire_timeout {
        0 => None,
        millis if millis < 0 => urgency.default_timeout(),
        millis => Some(Duration::from_millis(u64::from(millis.unsigned_abs()))),
    }
}

#[must_use]
pub fn sanitize_body(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        match ch {
            '<' if chars.peek().is_some_and(|next| next.is_ascii_alphabetic() || matches!(next, '/' | '!')) => {
                let mut tag = String::new();
                let mut closed = false;
                for inner in chars.by_ref() {
                    if inner == '>' {
                        closed = true;
                        break;
                    }
                    tag.push(inner);
                }
                if !closed {
                    out.push('<');
                    out.push_str(&tag);
                } else if tag_breaks_line(&tag) {
                    out.push('\n');
                }
            }
            '&' => {
                let mut entity = String::new();
                let mut terminated = false;
                while let Some(&inner) = chars.peek() {
                    if inner == ';' {
                        chars.next();
                        terminated = true;
                        break;
                    }
                    if entity.len() >= 8 || inner.is_whitespace() || inner == '&' {
                        break;
                    }
                    entity.push(inner);
                    chars.next();
                }
                out.push_str(&decode_entity(&entity, terminated));
            }
            other => out.push(other),
        }
    }

    collapse(&out)
}

#[must_use]
pub fn plain_text(raw: &str) -> String {
    collapse(raw)
}

fn tag_breaks_line(tag: &str) -> bool {
    let name: String = tag.trim_start_matches('/').chars().take_while(char::is_ascii_alphanumeric).collect();
    name.eq_ignore_ascii_case("br") || name.eq_ignore_ascii_case("tr")
}

fn decode_entity(entity: &str, terminated: bool) -> String {
    match entity {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        "nbsp" => " ".into(),
        other => decode_numeric_entity(other)
            .map_or_else(|| if terminated { format!("&{other};") } else { format!("&{other}") }, String::from),
    }
}

fn decode_numeric_entity(entity: &str) -> Option<char> {
    let digits = entity.strip_prefix('#')?;
    let code = match digits.strip_prefix(['x', 'X']) {
        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
        None => digits.parse().ok()?,
    };
    char::from_u32(code)
}

fn collapse(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pending_space = false;
    let mut pending_break = false;

    for ch in text.chars() {
        if ch == '\n' {
            pending_break = true;
            pending_space = false;
            continue;
        }
        if ch.is_whitespace() {
            pending_space = true;
            continue;
        }
        if !out.is_empty() {
            if pending_break {
                out.push('\n');
            } else if pending_space {
                out.push(' ');
            }
        }
        pending_break = false;
        pending_space = false;
        out.push(ch);
    }

    if out.chars().count() > MAX_BODY_CHARS {
        let kept: String = out.chars().take(MAX_BODY_CHARS - 1).collect();
        return format!("{}…", kept.trim_end());
    }
    out
}

#[must_use]
pub fn split_actions(raw: &[String]) -> (Vec<Action>, bool, bool, Option<String>) {
    let mut actions = Vec::new();
    let mut has_default = false;
    let mut has_settings = false;
    let mut reply_label = None;

    for pair in raw.chunks(2) {
        let label = pair.get(1).cloned().unwrap_or_default();
        match pair[0].as_str() {
            "" => {}
            ACTION_DEFAULT => has_default = true,
            ACTION_SETTINGS => has_settings = true,
            ACTION_INLINE_REPLY => {
                reply_label.get_or_insert(label);
            }
            key => actions.push(Action { key: key.to_string(), label }),
        }
    }

    (actions, has_default, has_settings, reply_label)
}

#[must_use]
pub fn capabilities() -> Vec<String> {
    [
        "actions",
        "body",
        "body-markup",
        "icon-static",
        "persistence",
        "sound",
        "inline-reply",
        "inhibitions",
        "x-kde-display-appname",
    ]
    .map(String::from)
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_reduced_to_plain_text() {
        assert_eq!(sanitize_body("<b>bold</b> text"), "bold text");
        assert_eq!(sanitize_body("a<br/>b"), "a\nb");
        assert_eq!(sanitize_body("<a href=\"http://x\">link</a>"), "link", "the label survives, the markup does not");
    }

    #[test]
    fn an_unclosed_tag_cannot_swallow_the_rest_of_the_body() {
        assert_eq!(sanitize_body("safe <img src=\"/evil"), "safe <img src=\"/evil");
        assert_eq!(sanitize_body("if x <5 then y"), "if x <5 then y");
        assert_eq!(sanitize_body("a < b > c"), "a < b > c");
    }

    #[test]
    fn plain_text_is_kept_verbatim_apart_from_whitespace() {
        assert_eq!(plain_text("<b>not bold</b> &amp;   co"), "<b>not bold</b> &amp; co");
    }

    #[test]
    fn entities_decode_to_their_characters() {
        assert_eq!(sanitize_body("a &amp; b"), "a & b");
        assert_eq!(sanitize_body("&lt;script&gt;"), "<script>");
        assert_eq!(sanitize_body("&#65;&#x42;"), "AB");
    }

    #[test]
    fn a_stray_ampersand_is_kept_rather_than_eating_the_text() {
        assert_eq!(sanitize_body("Tom & Jerry"), "Tom & Jerry");
        assert_eq!(sanitize_body("50% & rising"), "50% & rising");
    }

    #[test]
    fn an_unknown_entity_keeps_its_semicolon() {
        assert_eq!(sanitize_body("Q&A; see below"), "Q&A; see below");
        assert_eq!(sanitize_body("R&D;"), "R&D;");
        assert_eq!(sanitize_body("a &; b"), "a &; b");
    }

    #[test]
    fn decoded_entities_are_never_re_parsed_as_markup() {
        assert_eq!(
            sanitize_body("&lt;b&gt;not bold&lt;/b&gt;"),
            "<b>not bold</b>",
            "an escaped tag must stay visible text"
        );
    }

    #[test]
    fn runs_of_whitespace_collapse_but_line_breaks_survive() {
        assert_eq!(sanitize_body("a     b"), "a b");
        assert_eq!(sanitize_body("a\n\n\nb"), "a\nb");
        assert_eq!(sanitize_body("  padded  "), "padded");
    }

    #[test]
    fn a_runaway_body_is_truncated() {
        let body = "x".repeat(MAX_BODY_CHARS * 3);
        let clean = sanitize_body(&body);
        assert!(clean.chars().count() <= MAX_BODY_CHARS);
        assert!(clean.ends_with('…'));
    }

    #[test]
    fn timeouts_follow_the_specification() {
        for timeout in [5000, -1, 0] {
            assert_eq!(resolve_timeout(timeout, Urgency::Critical), None);
        }
        assert_eq!(resolve_timeout(0, Urgency::Normal), None);
        assert_eq!(resolve_timeout(-1, Urgency::Normal), Some(DEFAULT_TIMEOUT_NORMAL));
        assert_eq!(resolve_timeout(-1, Urgency::Low), Some(DEFAULT_TIMEOUT_LOW));
        assert_eq!(resolve_timeout(1234, Urgency::Normal), Some(Duration::from_millis(1234)));
        assert_eq!(CloseReason::Undefined.as_wire(), 4);
    }

    fn strings(words: &[&str]) -> Vec<String> {
        words.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn the_reserved_action_keys_are_pulled_out_of_the_button_list() {
        let raw = strings(&["default", "Open", "reply-now", "Reply", "settings", "Configure", "inline-reply", "Send"]);
        let (actions, has_default, has_settings, reply) = split_actions(&raw);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].key, "reply-now");
        assert!(has_default);
        assert!(has_settings);
        assert_eq!(reply.as_deref(), Some("Send"));
    }

    #[test]
    fn an_odd_action_list_does_not_panic() {
        let (actions, _, _, _) = split_actions(&strings(&["lonely"]));
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].label, "");
    }

    #[test]
    fn only_the_first_inline_reply_action_wins() {
        let raw = strings(&["inline-reply", "First", "inline-reply", "Second"]);
        let (_, _, _, reply) = split_actions(&raw);
        assert_eq!(reply.as_deref(), Some("First"));
    }

    #[test]
    fn grouping_prefers_the_desktop_entry_over_the_free_form_name() {
        let notification = Notification {
            desktop_entry: "org.telegram.desktop".to_string(),
            app_name: "Telegram Desktop".to_string(),
            ..Notification::default()
        };
        assert_eq!(notification.group_key(), "org.telegram.desktop");
    }

    #[test]
    fn grouping_falls_back_to_the_app_name_then_the_bus_name() {
        let named = Notification { app_name: "Mail".to_string(), ..Notification::default() };
        assert_eq!(named.group_key(), "mail");

        let anonymous = Notification { sender: ":1.42".to_string(), ..Notification::default() };
        assert_eq!(anonymous.group_key(), ":1.42");
    }

    #[test]
    fn an_identical_repeat_is_recognised_as_a_duplicate() {
        let first = Notification {
            summary: "Build finished".to_string(),
            app_name: "CI".to_string(),
            ..Notification::default()
        };
        let mut second = first.clone();
        second.id = 7;
        assert!(first.is_duplicate_of(&second));

        second.body = "now with detail".to_string();
        assert!(!first.is_duplicate_of(&second));
    }

    #[test]
    fn only_honoured_capabilities_are_advertised() {
        let advertised = capabilities();
        for unhonoured in ["body-hyperlinks", "x-kde-origin-name", "x-glide-progress"] {
            assert!(!advertised.iter().any(|cap| cap == unhonoured), "{unhonoured} is not shown by the shell");
        }
    }
}
