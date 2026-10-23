pub use crate::look::Lang;

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
}
