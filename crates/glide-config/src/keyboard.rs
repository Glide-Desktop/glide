type Parser = fn(&str) -> SystemKeyboard;

const NON_LATIN: [&str; 9] = ["ua", "ru", "by", "kz", "gr", "bg", "rs", "mk", "il"];

const LATIN_AMERICA: [&str; 19] =
    ["MX", "AR", "CO", "CL", "PE", "VE", "EC", "GT", "CU", "BO", "DO", "HN", "PY", "SV", "NI", "CR", "PR", "PA", "UY"];

const CONSOLE_IDENTITY: [&str; 37] = [
    "us", "de", "fr", "it", "es", "pt", "pl", "ru", "ua", "by", "kz", "cz", "sk", "hu", "ro", "dk", "no", "se", "fi",
    "is", "nl", "be", "gr", "tr", "lt", "lv", "ee", "hr", "si", "rs", "mk", "bg", "jp", "br", "ch", "ca", "il",
];

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

    fn native(layout: &str, variant: &str) -> Self {
        if NON_LATIN.contains(&layout) {
            let variant = if variant.is_empty() { String::new() } else { format!(",{variant}") };
            Self {
                layout: format!("us,{layout}"),
                variant,
                options: "grp:win_space_toggle".to_string(),
                ..Self::default()
            }
        } else {
            Self { layout: layout.to_string(), variant: variant.to_string(), ..Self::default() }
        }
    }

    #[must_use]
    pub fn for_locale(locale: &str) -> Self {
        let name = locale.split(['.', '@']).next().unwrap_or_default();
        let (language, territory) = name.split_once('_').unwrap_or((name, ""));
        let (layout, variant) = match (language, territory) {
            ("pt", "BR") => ("br", ""),
            ("de", "CH") => ("ch", ""),
            ("fr", "CH") => ("ch", "fr"),
            ("fr", "CA") => ("ca", ""),
            ("fr" | "nl", "BE") => ("be", ""),
            ("de", "AT") => ("at", ""),
            ("es", region) if LATIN_AMERICA.contains(&region) => ("latam", ""),
            ("sr", _) if locale.contains("@latin") => ("rs", "latin"),
            _ => ("", ""),
        };
        if !layout.is_empty() {
            return Self { layout: layout.to_string(), variant: variant.to_string(), ..Self::default() };
        }
        let native = match language {
            "uk" => "ua",
            "cs" => "cz",
            "sv" => "se",
            "el" => "gr",
            "da" => "dk",
            "nb" | "nn" => "no",
            "ja" => "jp",
            "be" => "by",
            "kk" => "kz",
            "et" => "ee",
            "sl" => "si",
            "sr" => "rs",
            "en" if locale.starts_with("en_GB") => "gb",
            "ru" | "de" | "fr" | "es" | "it" | "pl" | "pt" | "fi" | "tr" | "hu" | "sk" | "lv" | "lt" | "ro" | "hr"
            | "is" | "bg" | "mk" => language,
            _ => "us",
        };
        Self::native(native, "")
    }

    #[must_use]
    pub fn for_console_keymap(keymap: &str) -> Self {
        let file = keymap.trim().rsplit('/').next().unwrap_or_default().to_ascii_lowercase();
        let name = file.trim_end_matches(".gz").trim_end_matches(".map");
        let special = match name {
            "de_ch-latin1" | "de_ch" | "sg" | "sg-latin1" => Some(("ch", "")),
            "fr_ch-latin1" | "fr_ch" | "sf" | "sf-latin1" => Some(("ch", "fr")),
            "cf" => Some(("ca", "")),
            "us-acentos" => Some(("us", "intl")),
            "dvorak" => Some(("us", "dvorak")),
            "colemak" => Some(("us", "colemak")),
            "trf" => Some(("tr", "f")),
            "jp106" => Some(("jp", "")),
            "bg_pho-utf8" => Some(("bg", "phonetic")),
            _ => None,
        };
        if let Some((layout, variant)) = special {
            return Self::native(layout, variant);
        }
        let stem = name.split(['-', '_']).next().unwrap_or_default().trim_end_matches(|c: char| c.is_ascii_digit());
        let layout = match stem {
            "uk" => "gb",
            "sv" => "se",
            "et" => "ee",
            "croat" => "hr",
            "slovene" => "si",
            "sr" => "rs",
            "la" => "latam",
            "trq" => "tr",
            "ruwin" => "ru",
            "kazakh" => "kz",
            stem if CONSOLE_IDENTITY.contains(&stem) => stem,
            _ => return Self::default(),
        };
        let variant = if name.contains("nodeadkeys") {
            "nodeadkeys"
        } else if matches!(layout, "cz" | "sk") && name.contains("qwerty") && !name.contains("qwertz") {
            "qwerty"
        } else {
            ""
        };
        Self::native(layout, variant)
    }

    #[must_use]
    pub fn parse_vconsole_keymap(text: &str) -> Self {
        text.lines()
            .filter_map(|line| line.trim().split_once('='))
            .find(|(key, _)| key.trim() == "KEYMAP")
            .map(|(_, value)| Self::for_console_keymap(value.trim().trim_matches(['"', '\''])))
            .unwrap_or_default()
    }

    #[must_use]
    pub fn detect() -> Self {
        let from_env = Self::from_env(|key| std::env::var(key).ok().filter(|v| !v.is_empty()));
        if !from_env.layout.is_empty() {
            return from_env;
        }
        let parsers: [(&str, Parser); 4] = [
            ("/etc/X11/xorg.conf.d/00-keyboard.conf", Self::parse_xorg_conf),
            ("/etc/vconsole.conf", Self::parse_env_file),
            ("/etc/default/keyboard", Self::parse_env_file),
            ("/etc/vconsole.conf", Self::parse_vconsole_keymap),
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
    fn belarusian_and_kazakh_use_the_xkb_names_by_and_kz_with_a_switch_key() {
        for (locale, second) in [("be_BY.UTF-8", "by"), ("be_BY", "by"), ("kk_KZ.UTF-8", "kz"), ("kk", "kz")] {
            let keyboard = SystemKeyboard::for_locale(locale);
            assert_eq!(keyboard.layout, format!("us,{second}"), "{locale}");
            assert_eq!(keyboard.options, "grp:win_space_toggle", "{locale}");
        }
        assert_eq!(SystemKeyboard::for_locale("fr_BE.UTF-8").layout, "be", "Belgian stays Belgian");
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

    #[test]
    fn the_territory_picks_the_regional_layout() {
        let pick = |locale: &str| {
            let keyboard = SystemKeyboard::for_locale(locale);
            (keyboard.layout, keyboard.variant)
        };
        let expect = |layout: &str, variant: &str| (layout.to_string(), variant.to_string());
        assert_eq!(pick("pt_BR.UTF-8"), expect("br", ""));
        assert_eq!(pick("pt_PT.UTF-8"), expect("pt", ""));
        assert_eq!(pick("de_CH.UTF-8"), expect("ch", ""));
        assert_eq!(pick("fr_CH.UTF-8"), expect("ch", "fr"));
        assert_eq!(pick("fr_CA.UTF-8"), expect("ca", ""));
        assert_eq!(pick("fr_BE.UTF-8"), expect("be", ""));
        assert_eq!(pick("nl_BE.UTF-8@euro"), expect("be", ""));
        assert_eq!(pick("de_AT.UTF-8"), expect("at", ""));
        assert_eq!(pick("fr_FR.UTF-8"), expect("fr", ""));
        assert_eq!(pick("nl_NL.UTF-8"), expect("us", ""));
        assert_eq!(pick("uk_UA.UTF-8"), expect("us,ua", ""));
        assert_eq!(pick("es_MX.UTF-8"), expect("latam", ""));
        assert_eq!(pick("es_AR.UTF-8"), expect("latam", ""));
        assert_eq!(pick("es_ES.UTF-8"), expect("es", ""));
        assert_eq!(pick("sr_RS.UTF-8@latin"), expect("rs", "latin"));
    }

    #[test]
    fn languages_whose_xkb_names_differ_or_need_latin_beside_them_get_the_right_layout() {
        let layout = |locale: &str| SystemKeyboard::for_locale(locale).layout;
        for (locale, expected) in [
            ("sk_SK.UTF-8", "sk"),
            ("et_EE.UTF-8", "ee"),
            ("lv_LV.UTF-8", "lv"),
            ("lt_LT.UTF-8", "lt"),
            ("ro_RO.UTF-8", "ro"),
            ("hr_HR.UTF-8", "hr"),
            ("sl_SI.UTF-8", "si"),
            ("is_IS.UTF-8", "is"),
            ("bg_BG.UTF-8", "us,bg"),
            ("sr_RS.UTF-8", "us,rs"),
            ("mk_MK.UTF-8", "us,mk"),
        ] {
            assert_eq!(layout(locale), expected, "{locale}");
        }
        assert_eq!(SystemKeyboard::for_locale("bg_BG.UTF-8").options, "grp:win_space_toggle");
    }

    #[test]
    fn console_keymaps_are_read_by_their_stem_with_the_names_that_differ_from_xkb_mapped() {
        let pick = |keymap: &str| {
            let keyboard = SystemKeyboard::for_console_keymap(keymap);
            (keyboard.layout, keyboard.variant)
        };
        let expect = |layout: &str, variant: &str| (layout.to_string(), variant.to_string());
        assert_eq!(pick("de-latin1"), expect("de", ""));
        assert_eq!(pick("de-latin1-nodeadkeys"), expect("de", "nodeadkeys"));
        assert_eq!(pick("sk-qwertz"), expect("sk", ""));
        assert_eq!(pick("sk-qwerty"), expect("sk", "qwerty"));
        assert_eq!(pick("uk"), expect("gb", ""));
        assert_eq!(pick("sv-latin1"), expect("se", ""));
        assert_eq!(pick("et"), expect("ee", ""));
        assert_eq!(pick("la-latin1"), expect("latam", ""));
        assert_eq!(pick("br-abnt2"), expect("br", ""));
        assert_eq!(pick("pl2"), expect("pl", ""));
        assert_eq!(pick("sg"), expect("ch", ""));
        assert_eq!(pick("fr_CH-latin1"), expect("ch", "fr"));
        assert_eq!(pick("cf"), expect("ca", ""));
        assert_eq!(pick("croat"), expect("hr", ""));
        assert_eq!(pick("dvorak"), expect("us", "dvorak"));
        assert_eq!(pick("ua-utf"), expect("us,ua", ""));
        assert_eq!(pick("bg_pho-utf8"), expect("us,bg", ",phonetic"));
        assert_eq!(pick("/usr/share/kbd/keymaps/i386/qwerty/ru.map.gz"), expect("us,ru", ""));
        assert_eq!(pick("mystery-layout"), expect("", ""), "an unknown keymap falls through to the locale");
    }

    #[test]
    fn vconsole_keymap_is_used_only_when_no_xkb_key_is_set() {
        let text = "KEYMAP=\"de-latin1\"\nFONT=eurlatgr\n";
        assert_eq!(SystemKeyboard::parse_env_file(text).layout, "");
        assert_eq!(SystemKeyboard::parse_vconsole_keymap(text).layout, "de");
        assert_eq!(SystemKeyboard::parse_vconsole_keymap("FONT=x\n").layout, "");
    }
}
