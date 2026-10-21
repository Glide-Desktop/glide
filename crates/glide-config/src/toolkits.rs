use std::path::{Path, PathBuf};

const QT_PLUGIN_ROOTS: [&str; 9] = [
    "/usr/lib/qt6/plugins",
    "/usr/lib64/qt6/plugins",
    "/usr/lib/x86_64-linux-gnu/qt6/plugins",
    "/usr/lib/aarch64-linux-gnu/qt6/plugins",
    "/usr/lib/qt/plugins",
    "/usr/lib64/qt5/plugins",
    "/usr/lib/qt5/plugins",
    "/usr/lib/x86_64-linux-gnu/qt5/plugins",
    "/usr/lib/aarch64-linux-gnu/qt5/plugins",
];

pub const ADW_GTK3: &str = "adw-gtk3";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QtTheme {
    Qt6ct,
    Qt5ct,
    Gtk3,
}

impl QtTheme {
    #[must_use]
    pub const fn plugin(self) -> &'static str {
        match self {
            Self::Qt6ct => "qt6ct",
            Self::Qt5ct => "qt5ct",
            Self::Gtk3 => "gtk3",
        }
    }

    #[must_use]
    pub const fn is_qtct(self) -> bool {
        matches!(self, Self::Qt6ct | Self::Qt5ct)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Support {
    pub adw_gtk3: bool,
    pub kvantum: bool,
    pub qt: Option<QtTheme>,
}

impl Support {
    #[must_use]
    pub fn detect() -> Self {
        let themes: Vec<PathBuf> = crate::home()
            .map(|home| home.join(".themes"))
            .into_iter()
            .chain(crate::data_dirs().into_iter().map(|dir| dir.join("themes")))
            .collect();
        let plugins: Vec<PathBuf> = QT_PLUGIN_ROOTS.iter().map(PathBuf::from).collect();
        Self::detect_in(&themes, &plugins)
    }

    #[must_use]
    pub fn detect_in(themes: &[PathBuf], plugins: &[PathBuf]) -> Self {
        let plugin =
            |kind: &str, name: &str| plugins.iter().any(|root| root.join(kind).join(format!("lib{name}.so")).exists());
        let qt = [(QtTheme::Qt6ct, "qt6ct"), (QtTheme::Qt5ct, "qt5ct"), (QtTheme::Gtk3, "qgtk3")]
            .into_iter()
            .find(|(_, library)| plugin("platformthemes", library))
            .map(|(theme, _)| theme);
        Self {
            adw_gtk3: themes.iter().any(|root| gtk3_theme_in(root, ADW_GTK3)),
            kvantum: plugin("styles", "kvantum"),
            qt,
        }
    }

    #[must_use]
    pub fn qt_platform_theme(self) -> Option<&'static str> {
        self.qt.map(QtTheme::plugin)
    }

    #[must_use]
    pub fn qt_style_override(self) -> Option<&'static str> {
        (self.kvantum && !self.qt.is_some_and(QtTheme::is_qtct)).then_some("kvantum")
    }
}

fn gtk3_theme_in(root: &Path, name: &str) -> bool {
    root.join(name).join("gtk-3.0").is_dir()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("glide-toolkits-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, b"").expect("file");
    }

    #[test]
    fn qt_prefers_qt6ct_then_qt5ct_then_the_gtk3_bridge() {
        let root = scratch("qt");
        let plugins = [root.join("qt6"), root.join("qt5")];
        let detect = || Support::detect_in(&[], &plugins);
        assert_eq!(detect(), Support::default());
        assert_eq!(detect().qt_platform_theme(), None);

        touch(&plugins[0].join("platformthemes/libqgtk3.so"));
        assert_eq!(detect().qt_platform_theme(), Some("gtk3"));
        touch(&plugins[1].join("platformthemes/libqt5ct.so"));
        assert_eq!(detect().qt_platform_theme(), Some("qt5ct"));
        touch(&plugins[0].join("platformthemes/libqt6ct.so"));
        assert_eq!(detect().qt_platform_theme(), Some("qt6ct"));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn kvantum_is_forced_only_where_no_qtct_picks_the_style() {
        let root = scratch("kvantum");
        let plugins = [root.clone()];
        touch(&root.join("styles/libkvantum.so"));
        touch(&root.join("platformthemes/libqgtk3.so"));
        assert_eq!(Support::detect_in(&[], &plugins).qt_style_override(), Some("kvantum"));
        touch(&root.join("platformthemes/libqt6ct.so"));
        let support = Support::detect_in(&[], &plugins);
        assert!(support.kvantum && support.qt == Some(QtTheme::Qt6ct));
        assert_eq!(support.qt_style_override(), None);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn adw_gtk3_counts_only_with_its_gtk3_folder_in_any_theme_root() {
        let root = scratch("themes");
        let themes = [root.join("home/.themes"), root.join("share/themes")];
        std::fs::create_dir_all(themes[0].join(ADW_GTK3)).expect("dirs");
        assert!(!Support::detect_in(&themes, &[]).adw_gtk3, "a folder without gtk-3.0 is not a GTK 3 theme");
        std::fs::create_dir_all(themes[1].join(ADW_GTK3).join("gtk-3.0")).expect("dirs");
        assert!(Support::detect_in(&themes, &[]).adw_gtk3);
        let _ = std::fs::remove_dir_all(root);
    }
}
