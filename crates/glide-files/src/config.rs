use glide_theme::{Accent, Appearance};
use serde::Deserialize;

use crate::app::ViewMode;
use crate::fs::{SortBy, SortConfig};
use crate::i18n::Lang;

#[derive(Debug, Clone, PartialEq)]
pub struct Look {
    pub appearance: Appearance,
    pub appearance_schedule: bool,
    pub accent: Accent,
    pub language: Option<Lang>,
    pub icon_theme: Option<String>,
    pub font_family: Option<String>,
    pub motion_scale: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            appearance: Appearance::default(),
            appearance_schedule: false,
            accent: Accent::default(),
            language: None,
            icon_theme: None,
            font_family: None,
            motion_scale: 1.0,
        }
    }
}

impl Look {
    #[must_use]
    pub fn appearance_at(&self, minute_of_day: Option<u16>) -> Appearance {
        match minute_of_day.filter(|_| self.appearance_schedule) {
            Some(minute) => Appearance::scheduled(minute),
            None => self.appearance,
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct File {
    general: General,
    topbar: Topbar,
    dock: Dock,
    animations: Animations,
}

#[derive(Deserialize)]
#[serde(default, rename_all = "kebab-case")]
struct Animations {
    enabled: bool,
    speed: f32,
    respect_system: bool,
}

impl Default for Animations {
    fn default() -> Self {
        Self { enabled: true, speed: 1.0, respect_system: true }
    }
}

impl Animations {
    fn scale(&self, reduced_motion: bool) -> f32 {
        if !self.enabled || (self.respect_system && reduced_motion) { 0.0 } else { self.speed }
    }
}

#[must_use]
pub fn reduced_motion() -> bool {
    std::env::var("GLIDE_REDUCED_MOTION").is_ok_and(|value| value.trim() == "1")
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct General {
    language: Option<String>,
    font_family: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct Topbar {
    appearance: Appearance,
    appearance_schedule: bool,
    accent: Accent,
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "kebab-case")]
struct Dock {
    icon_theme: Option<String>,
}

impl Look {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        Self::parse_with(text, reduced_motion())
    }

    #[must_use]
    pub fn parse_with(text: &str, reduced_motion: bool) -> Self {
        let file: File = toml::from_str(text).unwrap_or_default();
        Self {
            motion_scale: file.animations.scale(reduced_motion),
            appearance: file.topbar.appearance,
            appearance_schedule: file.topbar.appearance_schedule,
            accent: file.topbar.accent,
            language: file.general.language.as_deref().and_then(Lang::from_key),
            icon_theme: file.dock.icon_theme.filter(|name| !name.trim().is_empty()),
            font_family: file.general.font_family.filter(|name| !name.trim().is_empty()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListColumn {
    Size,
    Modified,
    Created,
    Kind,
    Permissions,
    Tags,
}

impl ListColumn {
    pub const ALL: [Self; 6] = [Self::Size, Self::Modified, Self::Created, Self::Kind, Self::Permissions, Self::Tags];
    pub const DEFAULT: [Self; 3] = [Self::Size, Self::Modified, Self::Kind];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Size => "size",
            Self::Modified => "modified",
            Self::Created => "created",
            Self::Kind => "kind",
            Self::Permissions => "permissions",
            Self::Tags => "tags",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|column| column.key() == key.trim())
    }

    #[must_use]
    pub const fn sort(self) -> Option<SortBy> {
        match self {
            Self::Size => Some(SortBy::Size),
            Self::Modified => Some(SortBy::Modified),
            Self::Kind => Some(SortBy::Type),
            Self::Created | Self::Permissions | Self::Tags => None,
        }
    }
}

pub const GRID_ZOOMS: usize = 6;
pub const LIST_ZOOMS: usize = 4;
pub const DEFAULT_GRID_ZOOM: u8 = 2;
pub const DEFAULT_LIST_ZOOM: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ViewPrefs {
    pub view: ViewMode,
    pub sort: SortConfig,
    pub show_hidden: bool,
    pub grid_zoom: u8,
    pub list_zoom: u8,
    pub columns: Vec<ListColumn>,
    pub queue: bool,
}

impl Default for ViewPrefs {
    fn default() -> Self {
        Self {
            view: ViewMode::default(),
            sort: SortConfig::default(),
            show_hidden: false,
            grid_zoom: DEFAULT_GRID_ZOOM,
            list_zoom: DEFAULT_LIST_ZOOM,
            columns: ListColumn::DEFAULT.to_vec(),
            queue: false,
        }
    }
}

fn zoom(value: &str, levels: usize, fallback: u8) -> u8 {
    value.parse::<u8>().ok().filter(|level| usize::from(*level) < levels).unwrap_or(fallback)
}

impl ViewPrefs {
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut prefs = Self::default();
        for (key, value) in
            text.lines().filter_map(|line| line.split_once('=')).map(|(key, value)| (key.trim(), value.trim()))
        {
            match (key, value) {
                ("view", "list") => prefs.view = ViewMode::List,
                ("view", "grid") => prefs.view = ViewMode::Grid,
                ("sort", by) => {
                    if let Some(by) = SortBy::ALL.into_iter().find(|candidate| sort_key(*candidate) == by) {
                        prefs.sort.by = by;
                    }
                }
                ("ascending", flag) => prefs.sort.ascending = flag != "false",
                ("hidden", flag) => prefs.show_hidden = flag == "true",
                ("grid-zoom", level) => prefs.grid_zoom = zoom(level, GRID_ZOOMS, DEFAULT_GRID_ZOOM),
                ("list-zoom", level) => prefs.list_zoom = zoom(level, LIST_ZOOMS, DEFAULT_LIST_ZOOM),
                ("columns", list) => {
                    let mut columns: Vec<ListColumn> = Vec::new();
                    for column in list.split(',').filter_map(ListColumn::from_key) {
                        if !columns.contains(&column) {
                            columns.push(column);
                        }
                    }
                    prefs.columns = columns;
                }
                ("queue", flag) => prefs.queue = flag == "true",
                _ => {}
            }
        }
        prefs
    }

    #[must_use]
    pub fn render(&self) -> String {
        let view = match self.view {
            ViewMode::Grid => "grid",
            ViewMode::List => "list",
        };
        let columns: Vec<&str> = self.columns.iter().map(|column| column.key()).collect();
        format!(
            "view = {view}\nsort = {}\nascending = {}\nhidden = {}\ngrid-zoom = {}\nlist-zoom = {}\ncolumns = {}\nqueue = {}\n",
            sort_key(self.sort.by),
            self.sort.ascending,
            self.show_hidden,
            self.grid_zoom,
            self.list_zoom,
            columns.join(","),
            self.queue
        )
    }
}

const fn sort_key(by: SortBy) -> &'static str {
    match by {
        SortBy::Name => "name",
        SortBy::Size => "size",
        SortBy::Modified => "modified",
        SortBy::Type => "type",
    }
}

fn prefs_path() -> Option<std::path::PathBuf> {
    Some(glide_config::state_home()?.join("glide/files-view"))
}

#[must_use]
pub fn load_prefs() -> ViewPrefs {
    prefs_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| ViewPrefs::parse(&text))
        .unwrap_or_default()
}

pub fn save_prefs(prefs: &ViewPrefs) {
    let Some(path) = prefs_path() else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, prefs.render());
}

#[must_use]
pub fn path() -> Option<std::path::PathBuf> {
    glide_config::xdg_path("glide", "glide.toml")
}

#[must_use]
pub fn load() -> Look {
    path().and_then(|path| std::fs::read_to_string(path).ok()).map_or_else(
        || Look { motion_scale: Animations::default().scale(reduced_motion()), ..Look::default() },
        |text| Look::parse(&text),
    )
}
