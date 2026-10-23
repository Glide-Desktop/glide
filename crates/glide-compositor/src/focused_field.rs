use smithay::reexports::wayland_protocols::wp::text_input::zv3::server::zwp_text_input_v3::{
    ContentHint, ContentPurpose,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Text,
    Password,
    Pin,
    Code,
    Email,
    Url,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Field {
    pub kind: Kind,
    pub sensitive: bool,
}

impl Field {
    #[must_use]
    pub fn from_content(content: Option<(ContentHint, ContentPurpose)>) -> Self {
        let Some((hint, purpose)) = content else {
            return Self { kind: Kind::Text, sensitive: false };
        };
        let kind = match purpose {
            ContentPurpose::Password => Kind::Password,
            ContentPurpose::Pin => Kind::Pin,
            ContentPurpose::Digits | ContentPurpose::Number => Kind::Code,
            ContentPurpose::Email => Kind::Email,
            ContentPurpose::Url => Kind::Url,
            ContentPurpose::Terminal => Kind::Terminal,
            _ => Kind::Text,
        };
        let sensitive = matches!(kind, Kind::Password | Kind::Pin)
            || hint.intersects(ContentHint::SensitiveData | ContentHint::HiddenText);
        Self { kind, sensitive }
    }

    #[must_use]
    pub const fn takes_secret(self) -> bool {
        matches!(self.kind, Kind::Password | Kind::Pin) || (self.sensitive && !matches!(self.kind, Kind::Terminal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_field_without_a_content_type_is_plain_text() {
        assert_eq!(Field::from_content(None), Field { kind: Kind::Text, sensitive: false });
    }

    #[test]
    fn password_and_pin_purposes_take_secrets() {
        for purpose in [ContentPurpose::Password, ContentPurpose::Pin] {
            let field = Field::from_content(Some((ContentHint::None, purpose)));
            assert!(field.sensitive && field.takes_secret(), "{purpose:?}");
        }
    }

    #[test]
    fn a_hidden_text_hint_marks_a_normal_field_as_secret() {
        let field = Field::from_content(Some((ContentHint::HiddenText, ContentPurpose::Normal)));
        assert_eq!(field.kind, Kind::Text);
        assert!(field.takes_secret());
    }

    #[test]
    fn digit_fields_are_one_time_code_candidates_not_secrets() {
        let field = Field::from_content(Some((ContentHint::None, ContentPurpose::Digits)));
        assert_eq!(field.kind, Kind::Code);
        assert!(!field.takes_secret());
    }

    #[test]
    fn a_terminal_never_takes_a_secret_even_when_marked_sensitive() {
        let field = Field::from_content(Some((ContentHint::SensitiveData, ContentPurpose::Terminal)));
        assert!(!field.takes_secret());
    }
}
