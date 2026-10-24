pub use glide_sdk::look::Look;

use crate::app::ViewMode;
use crate::fs::{SortBy, SortConfig};

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

pub(crate) const GRID_ZOOMS: usize = 6;
pub(crate) const LIST_ZOOMS: usize = 4;
pub(crate) const DEFAULT_GRID_ZOOM: u8 = 2;
pub(crate) const DEFAULT_LIST_ZOOM: u8 = 1;

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
pub(crate) fn load_prefs() -> ViewPrefs {
    prefs_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .map(|text| ViewPrefs::parse(&text))
        .unwrap_or_default()
}

pub(crate) fn save_prefs(prefs: &ViewPrefs) {
    if let Some(path) = prefs_path() {
        let _ = glide_config::write_atomic(&path, prefs.render().as_bytes(), 0o644);
    }
}
