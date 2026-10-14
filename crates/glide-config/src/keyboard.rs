type Parser = fn(&str) -> SystemKeyboard;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemKeyboard {
    pub layout: String,
    pub variant: String,
    pub options: String,
    pub model: String,
}

impl SystemKeyboard {
    #[must_use]
    pub fn parse_xorg_conf(text: &str) -> Self {
        let mut keyboard = Self::default();
        for line in text.lines() {
            let mut quoted = line.split('"').skip(1).step_by(2);
            if line.trim_start().starts_with("Option")
                && let (Some(key), Some(value)) = (quoted.next(), quoted.next())
            {
                keyboard.set(key.trim_start_matches("Xkb"), value);
            }
        }
        keyboard
    }

    #[must_use]
    pub fn parse_env_file(text: &str) -> Self {
        let mut keyboard = Self::default();
        for (key, value) in text.lines().filter_map(|line| line.trim().split_once('=')) {
            if let Some(key) = key.trim().strip_prefix("XKB") {
                keyboard.set(key, value.trim().trim_matches('"').trim_matches('\''));
            }
        }
        keyboard
    }

    #[must_use]
    pub fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let mut keyboard = Self::default();
        for key in ["LAYOUT", "VARIANT", "OPTIONS", "MODEL"] {
            if let Some(value) = get(&format!("XKB_DEFAULT_{key}")) {
                keyboard.set(key, &value);
            }
        }
        keyboard
    }

    fn set(&mut self, key: &str, value: &str) {
        let slot = match key.to_ascii_lowercase().as_str() {
            "layout" => &mut self.layout,
            "variant" => &mut self.variant,
            "options" => &mut self.options,
            "model" => &mut self.model,
            _ => return,
        };
        value.trim().clone_into(slot);
    }

    #[must_use]
    pub fn for_locale(locale: &str) -> Self {
        let language = locale.split(['_', '.', '@']).next().unwrap_or_default();
        let native = match language {
            "uk" => "ua",
            "cs" => "cz",
            "sv" => "se",
            "el" => "gr",
            "da" => "dk",
            "nb" | "nn" => "no",
            "ja" => "jp",
            "en" if locale.starts_with("en_GB") => "gb",
            "ru" | "be" | "kk" | "de" | "fr" | "es" | "it" | "pl" | "pt" | "fi" | "tr" | "hu" => language,
            _ => "us",
        };
        if matches!(native, "ua" | "ru" | "be" | "kk" | "gr") {
            Self { layout: format!("us,{native}"), options: "grp:win_space_toggle".to_string(), ..Self::default() }
        } else {
            Self { layout: native.to_string(), ..Self::default() }
        }
    }

    #[must_use]
    pub fn detect() -> Self {
        let from_env = Self::from_env(|key| std::env::var(key).ok().filter(|v| !v.is_empty()));
        if !from_env.layout.is_empty() {
            return from_env;
        }
        let parsers: [(&str, Parser); 3] = [
            ("/etc/X11/xorg.conf.d/00-keyboard.conf", Self::parse_xorg_conf),
            ("/etc/vconsole.conf", Self::parse_env_file),
            ("/etc/default/keyboard", Self::parse_env_file),
        ];
        parsers
            .into_iter()
            .filter_map(|(path, parse)| std::fs::read_to_string(path).ok().map(|text| parse(&text)))
            .find(|keyboard| !keyboard.layout.is_empty())
            .unwrap_or_else(|| {
                let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
                    .into_iter()
                    .find_map(|key| std::env::var(key).ok().filter(|v| !v.is_empty()))
                    .unwrap_or_default();
                Self::for_locale(&locale)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn localed_xorg_snippet_is_read() {
        let text = r#"
Section "InputClass"
        Identifier "system-keyboard"
        MatchIsKeyboard "on"
        Option "XkbLayout" "de,us"
        Option "XkbVariant" "nodeadkeys,"
        Option "XkbOptions" "grp:alt_shift_toggle"
EndSection
"#;
        let keyboard = SystemKeyboard::parse_xorg_conf(text);
        assert_eq!(keyboard.layout, "de,us");
        assert_eq!(keyboard.variant, "nodeadkeys,");
        assert_eq!(keyboard.options, "grp:alt_shift_toggle");
        assert_eq!(keyboard.model, "");
    }

    #[test]
    fn vconsole_and_debian_keyboard_files_are_read() {
        let vconsole = "KEYMAP=fr\nXKBLAYOUT=fr\nXKBVARIANT=azerty\n";
        assert_eq!(SystemKeyboard::parse_env_file(vconsole).layout, "fr");
        let debian = "XKBMODEL=\"pc105\"\nXKBLAYOUT=\"us,ru\"\nXKBOPTIONS=\"grp:caps_toggle\"\n";
        let keyboard = SystemKeyboard::parse_env_file(debian);
        assert_eq!(keyboard.layout, "us,ru");
        assert_eq!(keyboard.model, "pc105");
        assert_eq!(keyboard.options, "grp:caps_toggle");
    }

    #[test]
    fn xkb_default_env_vars_are_read() {
        let keyboard = SystemKeyboard::from_env(|key| (key == "XKB_DEFAULT_LAYOUT").then(|| "pl".to_string()));
        assert_eq!(keyboard.layout, "pl");
        assert_eq!(keyboard.options, "");
    }

    #[test]
    fn cyrillic_locales_keep_latin_first_with_a_switch_key() {
        let keyboard = SystemKeyboard::for_locale("uk_UA.UTF-8");
        assert_eq!(keyboard.layout, "us,ua");
        assert_eq!(keyboard.options, "grp:win_space_toggle");
    }

    #[test]
    fn latin_locales_get_one_layout_and_unknown_ones_get_us() {
        assert_eq!(SystemKeyboard::for_locale("de_DE.UTF-8").layout, "de");
        assert_eq!(SystemKeyboard::for_locale("sv_SE.UTF-8").layout, "se");
        assert_eq!(SystemKeyboard::for_locale("en_GB.UTF-8").layout, "gb");
        assert_eq!(SystemKeyboard::for_locale("en_US.UTF-8").layout, "us");
        assert_eq!(SystemKeyboard::for_locale("C").layout, "us");
        assert_eq!(SystemKeyboard::for_locale("").layout, "us");
        assert!(SystemKeyboard::for_locale("de_DE").options.is_empty());
    }
}
