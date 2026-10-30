use glide_theme::{Color, DockTheme};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
#[derive(Default)]
pub struct DockConfig {
    #[serde(flatten)]
    pub theme: DockTheme,
    pub autohide: DockAutohide,
    pub items: Vec<DockItemConfig>,
    #[serde(skip)]
    pub pins: Vec<DockItemConfig>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DockAutohide {
    #[default]
    Smart,
    Always,
    Never,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct DockItemConfig {
    pub name: String,
    pub command: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub app_id: Option<String>,
    #[serde(default)]
    pub color_from: Option<Color>,
    #[serde(default)]
    pub color_to: Option<Color>,
    #[serde(default)]
    pub separator_before: bool,
}

impl DockItemConfig {
    #[must_use]
    pub fn installed_defaults() -> Vec<Self> {
        let mut seen = std::collections::HashSet::new();
        Self::defaults()
            .into_iter()
            .filter_map(|item| {
                let app = glide_shell::icon::find_application(item.app_id.as_deref()?)?;
                if !seen.insert(item.name.clone()) {
                    return None;
                }
                Some(Self { name: app.name, command: app.command, ..item })
            })
            .collect()
    }

    fn defaults() -> Vec<Self> {
        [
            ("Files", "glide-files", "files", "glide-files", false),
            ("Terminal", "alacritty", "terminal", "Alacritty", false),
            ("Settings", "glide-settings", "settings", "glide-settings", false),
            ("Music", "rhythmbox", "music", "rhythmbox", false),
            ("Mail", "thunderbird", "mail", "thunderbird", true),
        ]
        .into_iter()
        .map(|(name, command, icon, app_id, separator_before)| Self {
            name: name.to_string(),
            command: command.to_string(),
            icon: icon.to_string(),
            app_id: Some(app_id.to_string()),
            color_from: None,
            color_to: None,
            separator_before,
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dock_item_defaults_match_design_concept() {
        let defaults: Vec<_> = DockItemConfig::defaults()
            .into_iter()
            .map(|item| (item.name, item.command, item.icon, item.app_id.unwrap_or_default(), item.separator_before))
            .collect();
        let want = [
            ("Files", "glide-files", "files", "glide-files", false),
            ("Terminal", "alacritty", "terminal", "Alacritty", false),
            ("Settings", "glide-settings", "settings", "glide-settings", false),
            ("Music", "rhythmbox", "music", "rhythmbox", false),
            ("Mail", "thunderbird", "mail", "thunderbird", true),
        ]
        .map(|(n, c, i, a, s)| (n.to_string(), c.to_string(), i.to_string(), a.to_string(), s));
        assert_eq!(defaults, want);
    }
}
