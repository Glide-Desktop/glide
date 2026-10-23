use std::time::{Duration, Instant};

use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;

use crate::focused_field::Delivery;
use crate::i18n::{Key, t};
use crate::state::GlideState;

pub const OFFER_ID: u32 = u32::MAX - 9;
pub const INSERT: &str = "insert";
const FRESH: Duration = Duration::from_mins(3);
const MIN_DIGITS: usize = 4;
const MAX_DIGITS: usize = 8;

const WORDS: [&str; 20] = [
    "code",
    "codes",
    "код",
    "коду",
    "кодом",
    "коді",
    "кода",
    "kod",
    "otp",
    "pin",
    "passcode",
    "password",
    "пароль",
    "пароля",
    "2fa",
    "token",
    "login",
    "вхід",
    "входу",
    "вход",
];

const STEMS: [&str; 4] = ["verif", "підтвердж", "подтвержд", "одноразов"];

#[must_use]
pub fn extract(summary: &str, body: &str) -> Option<String> {
    let text: Vec<char> = format!("{summary}\n{body}").chars().collect();
    let keyword_char = keyword_start(&text)?;
    let candidates = candidates(&text);
    candidates
        .iter()
        .find(|(start, _)| *start >= keyword_char)
        .or_else(|| candidates.last())
        .map(|(_, code)| code.clone())
}

fn keyword_start(text: &[char]) -> Option<usize> {
    let lower: Vec<char> = text.iter().map(|ch| ch.to_lowercase().next().unwrap_or(*ch)).collect();
    let chars = |keyword: &&str| keyword.chars().collect::<Vec<char>>();
    let words: Vec<Vec<char>> = WORDS.iter().map(chars).collect();
    let stems: Vec<Vec<char>> = STEMS.iter().map(chars).collect();
    let ends_word = |at: usize| lower.get(at).is_none_or(|ch| !ch.is_alphanumeric());
    (0..lower.len()).find(|&at| {
        let at_word_start = at == 0 || !lower[at - 1].is_alphanumeric();
        let word = words.iter().any(|word| lower[at..].starts_with(word) && ends_word(at + word.len()));
        at_word_start && (word || stems.iter().any(|stem| lower[at..].starts_with(stem)))
    })
}

fn candidates(text: &[char]) -> Vec<(usize, String)> {
    let mut found = Vec::new();
    let mut index = 0;
    while index < text.len() {
        if !text[index].is_ascii_digit() {
            index += 1;
            continue;
        }
        let start = index;
        let mut groups = vec![run_length(text, index)];
        index += groups[0];
        while index + 1 < text.len()
            && matches!(text[index], ' ' | '-')
            && text[index + 1].is_ascii_digit()
            && matches!(groups.last(), Some(3 | 4))
        {
            let next = run_length(text, index + 1);
            if !matches!(next, 3 | 4) || groups.iter().sum::<usize>() + next > MAX_DIGITS {
                break;
            }
            groups.push(next);
            index += 1 + next;
        }
        let digits: String = text[start..index].iter().filter(|ch| ch.is_ascii_digit()).collect();
        if (MIN_DIGITS..=MAX_DIGITS).contains(&digits.len()) && standalone(text, start, index) {
            found.push((start, digits));
        }
    }
    found
}

fn run_length(text: &[char], from: usize) -> usize {
    text[from..].iter().take_while(|ch| ch.is_ascii_digit()).count()
}

fn standalone(text: &[char], start: usize, end: usize) -> bool {
    let before = start.checked_sub(1).map(|at| text[at]);
    let before_prev = start.checked_sub(2).map(|at| text[at]);
    let after = text.get(end).copied();
    let after_next = text.get(end + 1).copied();
    let glued = |ch: Option<char>| ch.is_some_and(char::is_alphanumeric);
    let joiner = |ch: Option<char>| matches!(ch, Some('.' | ',' | ':' | '/' | '-'));
    let digit = |ch: Option<char>| ch.is_some_and(|ch| ch.is_ascii_digit());
    let money_or_phone = before.is_some_and(|ch| matches!(ch, '+' | '$' | '€' | '£' | '₴' | '#'));
    let part_of_number = (joiner(after) && digit(after_next)) || (joiner(before) && digit(before_prev));
    let percent = after == Some('%');
    !glued(before) && !glued(after) && !money_or_phone && !part_of_number && !percent
}

#[derive(Debug, Clone)]
pub struct Code {
    digits: String,
    app: String,
    at: Instant,
    offered_to: Option<WlSurface>,
}

impl GlideState {
    pub fn remember_code(&mut self, notification: &glide_notify::Notification) {
        if crate::notifications::is_local(notification.id) {
            return;
        }
        let Some(digits) = extract(&notification.summary, &notification.body) else {
            return;
        };
        tracing::info!(app = notification.display_app(), "a one-time code arrived in a notification");
        self.session.otp =
            Some(Code { digits, app: notification.display_app().to_string(), at: Instant::now(), offered_to: None });
        self.offer_code();
    }

    pub fn offer_code(&mut self) {
        let now = Instant::now();
        if self.session.otp.as_ref().is_some_and(|code| now.saturating_duration_since(code.at) > FRESH) {
            self.withdraw_code();
            return;
        }
        let field_takes_code = self.session.text_field.is_some_and(crate::focused_field::Field::takes_code);
        let (Some(surface), true) = (self.focused_surface(), field_takes_code) else {
            return;
        };
        let Some(code) = self.session.otp.as_mut().filter(|code| code.offered_to.as_ref() != Some(&surface)) else {
            return;
        };
        code.offered_to = Some(surface);
        let notice = glide_notify::Notification {
            id: OFFER_ID,
            app_name: "Glide".to_string(),
            app_icon: "dialog-password".to_string(),
            summary: t(Key::CodeOffer).replace("{app}", &code.app).replace("{code}", &code.digits),
            body: t(Key::CodeOfferBody).to_string(),
            actions: vec![glide_notify::Action { key: INSERT.into(), label: t(Key::CodeInsert).into() }],
            transient: true,
            timeout: Some(FRESH.saturating_sub(now.saturating_duration_since(code.at))),
            ..glide_notify::Notification::default()
        };
        self.notifications.push(&mut self.dock.icons, notice, true);
        self.queue_redraw();
    }

    pub fn insert_code(&mut self) {
        let Some(code) = self.session.otp.clone() else {
            return;
        };
        let fresh = Instant::now().saturating_duration_since(code.at) <= FRESH;
        let delivered = match code.offered_to.as_ref().filter(|_| fresh) {
            Some(surface) => self.deliver_text(surface, &code.digits),
            None => Delivery::Refused,
        };
        tracing::info!(?delivered, app = code.app, "one-time code insertion");
        if delivered != Delivery::Refused || !fresh {
            self.withdraw_code();
        }
    }

    fn withdraw_code(&mut self) {
        self.session.otp = None;
        self.notifications.dismiss(OFFER_ID);
        self.queue_redraw();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_code_is_found_after_its_keyword() {
        assert_eq!(extract("Telegram", "Login code: 48291. Do not give this code to anyone"), Some("48291".into()));
        assert_eq!(extract("Банк", "Ваш код підтвердження 123 456"), Some("123456".into()));
        assert_eq!(extract("Google", "G-123456 is your Google verification code"), Some("123456".into()));
    }

    #[test]
    fn grouped_digits_join_only_in_threes_and_fours() {
        assert_eq!(extract("", "Your code: 1234 5678"), Some("12345678".into()));
        assert_eq!(extract("", "Your code: 482-913"), Some("482913".into()));
    }

    #[test]
    fn numbers_without_a_code_keyword_are_ignored() {
        assert_eq!(extract("Calendar", "Meeting at 1530 in room 4012"), None);
    }

    #[test]
    fn a_keyword_inside_another_word_does_not_count() {
        assert_eq!(extract("Shop", "Shipping update: parcel 48213 is on its way"), None);
        assert_eq!(extract("Shop", "Your barcode 48213 is attached"), None);
        assert_eq!(extract("Player", "Кодек 2160 підтримується"), None);
        assert_eq!(extract("Банк", "Не повідомляйте коду 5521 нікому"), Some("5521".into()));
        assert_eq!(extract("Bank", "Your verification number: 5521"), Some("5521".into()), "stems still match");
    }

    #[test]
    fn phones_amounts_times_and_dates_are_not_codes() {
        assert_eq!(extract("", "Call +380671234567 for your code"), None);
        assert_eq!(extract("", "Code for payment of $1500"), None);
        assert_eq!(extract("", "Код дійсний до 12:30, 24/10/2026"), None);
        assert_eq!(extract("", "Your code 12.5% discount"), None);
        assert_eq!(extract("", "Your code: call 067-123-4567"), None);
    }

    #[test]
    fn the_first_number_after_the_keyword_wins() {
        assert_eq!(extract("Shop", "Order 55512: your code is 7391"), Some("7391".into()));
    }

    #[test]
    fn digits_glued_to_letters_are_not_a_code() {
        assert_eq!(extract("", "code AB12345CD"), None);
    }
}
