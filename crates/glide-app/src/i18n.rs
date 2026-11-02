const LOCALE_VARS: [&str; 3] = ["LC_ALL", "LC_MESSAGES", "LANG"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "uk" | "ua" | "ukrainian" => Some(Self::Uk),
            "en" | "english" => Some(Self::En),
            _ => None,
        }
    }

    #[must_use]
    pub fn from_locale(locale: &str) -> Self {
        if locale.is_empty() || locale.starts_with("uk") { Self::Uk } else { Self::En }
    }

    #[must_use]
    pub fn from_locale_vars(lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>) -> Self {
        let locale = [lc_all, lc_messages, lang].into_iter().flatten().map(str::trim).find(|value| !value.is_empty());
        Self::from_locale(locale.unwrap_or_default())
    }

    #[must_use]
    pub fn from_env() -> Self {
        let [lc_all, lc_messages, lang] = LOCALE_VARS.map(|key| std::env::var(key).ok());
        Self::from_locale_vars(lc_all.as_deref(), lc_messages.as_deref(), lang.as_deref())
    }

    #[must_use]
    pub fn from_env_or(fallback: Self) -> Self {
        let [lc_all, lc_messages, lang] = LOCALE_VARS.map(|key| std::env::var(key).ok());
        Self::from_locale_vars_or([lc_all.as_deref(), lc_messages.as_deref(), lang.as_deref()], fallback)
    }

    #[must_use]
    pub fn from_locale_vars_or(values: [Option<&str>; 3], fallback: Self) -> Self {
        values
            .into_iter()
            .flatten()
            .map(str::trim)
            .find(|value| !value.is_empty())
            .and_then(Self::naming)
            .unwrap_or(fallback)
    }

    fn naming(locale: &str) -> Option<Self> {
        let lower = locale.trim().to_ascii_lowercase();
        if lower.starts_with("uk") {
            Some(Self::Uk)
        } else if lower.starts_with("en") {
            Some(Self::En)
        } else {
            None
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Uk => "uk",
            Self::En => "en",
        }
    }

    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Uk => "Українська",
            Self::En => "English",
        }
    }

    #[must_use]
    pub const fn plural(self, count: u64) -> Plural {
        match self {
            Self::En if count == 1 => Plural::One,
            Self::En => Plural::Many,
            Self::Uk => match (count % 10, count % 100) {
                (1, n) if n != 11 => Plural::One,
                (2..=4, n) if n < 12 || n > 14 => Plural::Few,
                _ => Plural::Many,
            },
        }
    }

    #[must_use]
    pub fn pick<'a>(self, uk: &'a str, en: &'a str) -> &'a str {
        match self {
            Self::Uk => uk,
            Self::En => en,
        }
    }

    #[must_use]
    pub const fn weekday(self, from_sunday: usize) -> &'static str {
        let names = match self {
            Self::Uk => ["неділя", "понеділок", "вівторок", "середа", "четвер", "п\u{2019}ятниця", "субота"],
            Self::En => ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"],
        };
        names[from_sunday % 7]
    }

    #[must_use]
    pub const fn month_of_date(self, month0: usize) -> &'static str {
        let names = match self {
            Self::Uk => [
                "січня",
                "лютого",
                "березня",
                "квітня",
                "травня",
                "червня",
                "липня",
                "серпня",
                "вересня",
                "жовтня",
                "листопада",
                "грудня",
            ],
            Self::En => [
                "January",
                "February",
                "March",
                "April",
                "May",
                "June",
                "July",
                "August",
                "September",
                "October",
                "November",
                "December",
            ],
        };
        names[month0 % 12]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Plural {
    One,
    Few,
    Many,
}

pub trait Text: Copy + std::fmt::Debug + 'static {
    fn en(self) -> &'static str;

    fn uk(self) -> &'static str;

    fn written(self) -> bool {
        false
    }

    fn text(self, lang: Lang) -> &'static str {
        match lang {
            Lang::Uk => self.uk(),
            Lang::En => self.en(),
        }
    }
}

#[must_use]
pub fn count(count: u64, lang: Lang, [one, few, many]: [&str; 3]) -> String {
    let noun = match lang.plural(count) {
        Plural::One => one,
        Plural::Few => few,
        Plural::Many => many,
    };
    format!("{count} {noun}")
}

#[must_use]
pub fn ago(seconds: u64, lang: Lang) -> String {
    let (amount, unit) = match seconds {
        0..60 => return lang.pick("щойно", "just now").to_string(),
        60..3600 => (seconds / 60, lang.pick("хв", "min")),
        3600..86_400 => (seconds / 3600, lang.pick("год", "h")),
        _ => {
            let days = seconds / 86_400;
            let noun = match (lang, lang.plural(days)) {
                (Lang::En, Plural::One) => "day",
                (Lang::En, _) => "days",
                (Lang::Uk, Plural::One) => "день",
                (Lang::Uk, Plural::Few) => "дні",
                (Lang::Uk, Plural::Many) => "днів",
            };
            (days, noun)
        }
    };
    match lang {
        Lang::Uk => format!("{amount} {unit} тому"),
        Lang::En => format!("{amount} {unit} ago"),
    }
}

#[must_use]
pub fn problems<K: Text>(keys: &[K]) -> Vec<String> {
    let mut found = Vec::new();
    for key in keys {
        for text in [key.en(), key.uk()] {
            if text.trim().is_empty() {
                found.push(format!("{key:?} has an empty translation"));
            }
            let letters: Vec<char> = text.chars().filter(|c| c.is_alphabetic()).collect();
            if !key.written() && letters.len() > 3 && letters.iter().all(|c| c.is_uppercase()) {
                found.push(format!("{key:?} is written in capitals: {text}"));
            }
        }
    }
    found
}

#[macro_export]
macro_rules! catalogue {
    (@written) => {
        false
    };
    (@written written) => {
        true
    };
    ($($key:ident => $en:literal, $uk:literal $(as $written:ident)?;)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(clippy::enum_variant_names)] // Keys name UI strings such as "ComposeKey", not variants of a key
        pub enum Key {
            $($key,)*
        }

        impl Key {
            #[allow(dead_code)] // Read by the generated test and by catalogues that list every key
            pub const ALL: &'static [Self] = &[$(Self::$key,)*];

            #[must_use]
            pub const fn en(self) -> &'static str {
                match self {
                    $(Self::$key => $en,)*
                }
            }

            #[must_use]
            pub const fn uk(self) -> &'static str {
                match self {
                    $(Self::$key => $uk,)*
                }
            }

            #[must_use]
            pub const fn text(self, lang: $crate::i18n::Lang) -> &'static str {
                match lang {
                    $crate::i18n::Lang::Uk => self.uk(),
                    $crate::i18n::Lang::En => self.en(),
                }
            }
        }

        impl $crate::i18n::Text for Key {
            fn en(self) -> &'static str {
                Key::en(self)
            }

            fn uk(self) -> &'static str {
                Key::uk(self)
            }

            fn written(self) -> bool {
                match self {
                    $(Self::$key => $crate::catalogue!(@written $($written)?),)*
                }
            }
        }

        #[cfg(test)]
        mod catalogue_tests {
            #[test]
            fn every_key_is_translated_and_none_shouts() {
                let problems = $crate::i18n::problems(super::Key::ALL);
                assert!(problems.is_empty(), "{problems:#?}");
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    crate::catalogue! {
        Save => "Save", "Зберегти";
        Open => "Open…", "Відкрити…";
        Upper => "UPPERCASE", "ВЕЛИКІ ЛІТЕРИ" as written;
    }

    #[derive(Debug, Clone, Copy)]
    struct Shout;

    impl Text for Shout {
        fn en(self) -> &'static str {
            "WARNING"
        }

        fn uk(self) -> &'static str {
            ""
        }
    }

    #[test]
    fn a_catalogue_picks_the_language_and_flags_capitals_and_gaps() {
        assert_eq!(Key::Save.text(Lang::Uk), "Зберегти");
        assert_eq!(Text::text(Key::Open, Lang::En), "Open…");
        assert_eq!(Key::ALL, [Key::Save, Key::Open, Key::Upper]);
        assert!(Text::written(Key::Upper) && !Text::written(Key::Save));
        assert_eq!(problems(&[Shout]), ["Shout is written in capitals: WARNING", "Shout has an empty translation"]);
    }

    #[test]
    fn counts_take_the_plural_form_of_their_language() {
        let files = ["файл", "файли", "файлів"];
        assert_eq!(count(1, Lang::Uk, files), "1 файл");
        assert_eq!(count(3, Lang::Uk, files), "3 файли");
        assert_eq!(count(11, Lang::Uk, files), "11 файлів");
        assert_eq!(count(2, Lang::En, ["file", "files", "files"]), "2 files");
    }

    #[test]
    fn ago_reads_as_a_short_phrase_in_either_language() {
        assert_eq!(ago(20, Lang::Uk), "щойно");
        assert_eq!(ago(5 * 60, Lang::Uk), "5 хв тому");
        assert_eq!(ago(3 * 3600, Lang::En), "3 h ago");
        assert_eq!(ago(2 * 86_400, Lang::Uk), "2 дні тому");
        assert_eq!(ago(5 * 86_400, Lang::Uk), "5 днів тому");
        assert_eq!(ago(86_400, Lang::En), "1 day ago");
    }

    #[test]
    fn a_ukrainian_or_unset_locale_is_ukrainian() {
        assert_eq!(Lang::from_locale(""), Lang::Uk);
        assert_eq!(Lang::from_locale("uk_UA.UTF-8"), Lang::Uk);
        assert_eq!(Lang::from_locale("en_US.UTF-8"), Lang::En);
        assert_eq!(Lang::from_locale("de_DE.UTF-8"), Lang::En);
    }

    #[test]
    fn the_first_set_locale_variable_decides() {
        let pick = Lang::from_locale_vars;
        assert_eq!(pick(None, None, Some("uk_UA.UTF-8")), Lang::Uk);
        assert_eq!(pick(None, None, Some("en_US.UTF-8")), Lang::En);
        assert_eq!(pick(None, None, None), Lang::Uk);
        assert_eq!(pick(Some("C"), None, Some("uk_UA.UTF-8")), Lang::En);
        assert_eq!(pick(Some(" "), Some("uk_UA"), Some("en_GB")), Lang::Uk);
        assert_eq!(pick(None, Some("de_DE"), Some("uk_UA")), Lang::En);
    }

    #[test]
    fn with_a_fallback_the_first_set_variable_still_decides_and_other_languages_fall_back() {
        let pick = |values| Lang::from_locale_vars_or(values, Lang::Uk);
        assert_eq!(pick([Some("C"), None, Some("en_US.UTF-8")]), Lang::Uk);
        assert_eq!(Lang::from_locale_vars_or([Some("C"), None, Some("uk_UA.UTF-8")], Lang::En), Lang::En);
        assert_eq!(pick([Some(""), Some("en_GB"), Some("uk_UA")]), Lang::En);
        assert_eq!(pick([None, None, Some("de_DE")]), Lang::Uk);
        assert_eq!(pick([None, None, None]), Lang::Uk);
    }

    #[test]
    fn ukrainian_plurals_follow_the_last_two_digits() {
        use Plural::{Few, Many, One};
        let uk = |count| Lang::Uk.plural(count);
        assert_eq!([1, 21, 101].map(uk), [One; 3]);
        assert_eq!([2, 4, 22, 104].map(uk), [Few; 4]);
        assert_eq!([0, 5, 11, 12, 14, 111, 112].map(uk), [Many; 7]);
        assert_eq!([1, 0, 2, 21].map(|count| Lang::En.plural(count)), [One, Many, Many, Many]);
    }
}
