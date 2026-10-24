use std::path::{Path, PathBuf};
use std::time::SystemTime;

use toml_edit::{ArrayOfTables, DocumentMut, Item, Table, value};

use crate::i18n::Lang;
use crate::theme::{Accent, Appearance};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WallpaperFit {
    #[default]
    Cover,
    Contain,
    Center,
    Tile,
}

impl WallpaperFit {
    pub const ALL: &'static [Self] = &[Self::Cover, Self::Contain, Self::Center, Self::Tile];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Contain => "contain",
            Self::Center => "center",
            Self::Tile => "tile",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|fit| fit.key() == key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccelProfile {
    #[default]
    Adaptive,
    Flat,
}

impl AccelProfile {
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Adaptive => "adaptive",
            Self::Flat => "flat",
        }
    }
}

#[must_use]
pub(crate) const fn appearance_key(appearance: Appearance) -> &'static str {
    match appearance {
        Appearance::Light => "light",
        Appearance::Dark => "dark",
    }
}

#[must_use]
pub const fn accent_key(accent: Accent) -> &'static str {
    match accent {
        Accent::Azurit => "blue",
        Accent::Malachite => "teal",
        Accent::Conifer => "green",
        Accent::Amber => "amber",
        Accent::Terracotta => "orange",
        Accent::Cinnabar => "red",
        Accent::Amethyst => "purple",
        Accent::Graphite => "graphite",
    }
}

#[must_use]
pub fn parse_clock(text: &str) -> Option<u16> {
    let (hours, minutes) = text.trim().split_once(':')?;
    if minutes.len() != 2 {
        return None;
    }
    let hours: u16 = hours.parse().ok()?;
    let minutes: u16 = minutes.parse().ok()?;
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

#[must_use]
pub fn parse_error_summary(text: &str, message: &str, offset: Option<usize>) -> String {
    let message = message.trim();
    let Some(offset) = offset.filter(|offset| text.is_char_boundary(*offset)) else {
        return message.to_string();
    };
    let before = &text[..offset];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().unwrap_or("").chars().count() + 1;
    format!("line {line}, column {column}: {message}")
}

fn float(item: &Item) -> Option<f64> {
    item.as_float().or_else(|| item.as_integer().map(|v| v as f64))
}

fn bare(value: &toml_edit::Value) -> String {
    value.clone().decorated("", "").to_string()
}

#[must_use]
pub fn round_to(value: f64, step: f64) -> f64 {
    (value / step).round() * step
}

fn system_keyboard() -> &'static glide_config::SystemKeyboard {
    static KEYBOARD: std::sync::OnceLock<glide_config::SystemKeyboard> = std::sync::OnceLock::new();
    KEYBOARD.get_or_init(glide_config::SystemKeyboard::detect)
}
pub(crate) const DEFAULT_REPEAT_DELAY: i64 = 200;
pub(crate) const DEFAULT_REPEAT_RATE: i64 = 25;
pub const DEFAULT_ITEM_SIZE: i64 = 56;
pub const DEFAULT_ICON_SIZE: i64 = 44;
pub(crate) const DEFAULT_TITLEBAR_HEIGHT: i64 = 20;
pub(crate) const DEFAULT_BORDER_WIDTH: i64 = 1;
pub(crate) const DEFAULT_BLANK_AFTER: i64 = 600;
pub(crate) const DEFAULT_BATTERY_LOW: i64 = 20;
pub(crate) const DEFAULT_MAX_VOLUME: i64 = 100;
pub(crate) const DEFAULT_NIGHT_LIGHT_KELVIN: i64 = 4000;

pub const SPEED_MIN: f64 = 0.5;
pub const SPEED_MAX: f64 = 2.0;

#[must_use]
pub fn icon_size_for(item_size: i64) -> i64 {
    let exact = item_size * DEFAULT_ICON_SIZE;
    let nearest = (exact + DEFAULT_ITEM_SIZE / 2) / DEFAULT_ITEM_SIZE;
    if (item_size - nearest) % 2 == 0 {
        return nearest;
    }
    let below = nearest - 1;
    let above = nearest + 1;
    if (above * DEFAULT_ITEM_SIZE - exact).abs() < (below * DEFAULT_ITEM_SIZE - exact).abs() { above } else { below }
}

#[must_use]
pub fn speed_from_multiplier(multiplier: f64) -> f64 {
    if !multiplier.is_finite() || multiplier <= 0.0 {
        return SPEED_MAX;
    }
    round_to((1.0 / multiplier).clamp(SPEED_MIN, SPEED_MAX), 0.05)
}

#[must_use]
pub fn multiplier_from_speed(speed: f64) -> f64 {
    let speed = if speed.is_finite() { speed } else { 1.0 };
    round_to(1.0 / speed.clamp(SPEED_MIN, SPEED_MAX), 0.01)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFault {
    Io,
    Parse,
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|meta| meta.modified()).ok()
}

#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: Option<PathBuf>,
    doc: DocumentMut,
    error: Option<(ConfigFault, String)>,
    dirty: bool,
    modified: Option<SystemTime>,
    system_language: Lang,
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self {
            path: None,
            doc: DocumentMut::new(),
            error: None,
            dirty: false,
            modified: None,
            system_language: Lang::from_env(),
        }
    }
}

impl ConfigStore {
    #[must_use]
    pub fn load() -> Self {
        match glide_config::xdg_path("glide", "glide.toml") {
            Some(path) => Self::load_from(&path),
            None => Self::default(),
        }
    }

    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let stamp = modified(path);
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(err) => {
                return Self {
                    path: Some(path.to_path_buf()),
                    error: Some((ConfigFault::Io, err.to_string())),
                    modified: stamp,
                    ..Self::default()
                };
            }
        };
        let mut store = Self::parse(&text);
        store.path = Some(path.to_path_buf());
        store.modified = stamp;
        store
    }

    #[must_use]
    pub fn parse(text: &str) -> Self {
        match text.parse::<DocumentMut>() {
            Ok(doc) => Self { doc, ..Self::default() },
            Err(err) => Self {
                error: Some((
                    ConfigFault::Parse,
                    parse_error_summary(text, err.message(), err.span().map(|span| span.start)),
                )),
                ..Self::default()
            },
        }
    }

    pub fn refresh_if_changed(&mut self) -> bool {
        let Some(path) = self.path.clone() else {
            return false;
        };
        if self.dirty || modified(&path) == self.modified {
            return false;
        }
        let system_language = self.system_language;
        *self = Self::load_from(&path).with_system_language(system_language);
        true
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(|(_, message)| message.as_str())
    }

    #[must_use]
    pub fn fault(&self) -> Option<ConfigFault> {
        self.error.as_ref().map(|(fault, _)| *fault)
    }

    #[must_use]
    pub const fn writable(&self) -> bool {
        self.error.is_none()
    }

    pub fn take_dirty(&mut self) -> bool {
        std::mem::take(&mut self.dirty)
    }

    #[must_use]
    pub fn to_text(&self) -> String {
        self.doc.to_string()
    }

    pub fn save(&mut self) -> Result<(), String> {
        if let Some(message) = self.error() {
            return Err(message.to_string());
        }
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        let target = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if let Some(dir) = target.parent() {
            std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
        write_atomic(&target, self.doc.to_string().as_bytes()).map_err(|err| err.to_string())?;
        self.modified = modified(&path);
        Ok(())
    }

    fn item(&self, table: &str, key: &str) -> Option<&Item> {
        self.doc.get(table)?.get(key)
    }

    #[must_use]
    pub fn get_str(&self, table: &str, key: &str) -> Option<&str> {
        self.item(table, key)?.as_str()
    }

    #[must_use]
    pub fn get_bool(&self, table: &str, key: &str) -> Option<bool> {
        self.item(table, key)?.as_bool()
    }

    #[must_use]
    pub fn get_int(&self, table: &str, key: &str) -> Option<i64> {
        self.item(table, key)?.as_integer()
    }

    #[must_use]
    pub fn get_float(&self, table: &str, key: &str) -> Option<f64> {
        float(self.item(table, key)?)
    }

    fn table_mut(&mut self, name: &str) -> &mut dyn toml_edit::TableLike {
        let item = self.doc.as_table_mut().entry(name).or_insert_with(|| Item::Table(Table::new()));
        if item.as_table_like().is_none() {
            *item = Item::Table(Table::new());
        }
        item.as_table_like_mut().expect("the item was just made a table")
    }

    fn set(&mut self, table: &str, key: &str, mut item: Item) {
        if self.error.is_some() {
            return;
        }
        let target = self.table_mut(table);
        let old = target.get(key).and_then(Item::as_value);
        if old.map(bare) == item.as_value().map(bare) {
            return;
        }
        if let (Some(old), Some(new)) = (old, item.as_value_mut()) {
            *new.decor_mut() = old.decor().clone();
        }
        target.insert(key, item);
        self.dirty = true;
    }

    pub(crate) fn set_str(&mut self, table: &str, key: &str, text: &str) {
        self.set(table, key, value(text));
    }

    pub fn set_bool(&mut self, table: &str, key: &str, flag: bool) {
        self.set(table, key, value(flag));
    }

    pub(crate) fn set_int(&mut self, table: &str, key: &str, number: i64) {
        self.set(table, key, value(number));
    }

    pub fn set_float(&mut self, table: &str, key: &str, number: f64) {
        self.set(table, key, value(round_to(number, 0.01)));
    }

    #[must_use]
    pub fn get_str_array(&self, table: &str, key: &str) -> Vec<String> {
        self.item(table, key)
            .and_then(Item::as_array)
            .map(|array| array.iter().filter_map(|value| value.as_str().map(str::to_string)).collect())
            .unwrap_or_default()
    }

    pub fn set_str_array(&mut self, table: &str, key: &str, values: &[String]) {
        let array: toml_edit::Array = values.iter().map(String::as_str).collect();
        self.set(table, key, value(array));
    }

    #[must_use]
    pub(crate) fn int_or(&self, table: &str, key: &str, fallback: i64) -> i64 {
        self.get_int(table, key).unwrap_or(fallback)
    }

    #[must_use]
    pub(crate) fn bool_or(&self, table: &str, key: &str, fallback: bool) -> bool {
        self.get_bool(table, key).unwrap_or(fallback)
    }

    #[must_use]
    pub(crate) fn str_or(&self, table: &str, key: &str, fallback: &str) -> String {
        self.get_str(table, key).unwrap_or(fallback).to_string()
    }

    #[must_use]
    pub fn optional_str(&self, table: &str, key: &str) -> Option<String> {
        self.get_str(table, key).filter(|text| !text.trim().is_empty()).map(str::to_string)
    }

    pub fn set_optional_str(&mut self, table: &str, key: &str, text: Option<&str>) {
        match text.filter(|text| !text.trim().is_empty()) {
            Some(text) => self.set_str(table, key, text),
            None => self.remove(table, key),
        }
    }

    #[must_use]
    pub fn autostart(&self) -> Vec<String> {
        self.get_str_array("general", "autostart")
    }

    pub fn remove(&mut self, table: &str, key: &str) {
        if self.error.is_some() {
            return;
        }
        if let Some(target) = self.doc.get_mut(table).and_then(Item::as_table_like_mut)
            && target.remove(key).is_some()
        {
            self.dirty = true;
        }
    }

    #[must_use]
    pub fn appearance(&self) -> Appearance {
        match self.get_str("topbar", "appearance") {
            Some("light") => Appearance::Light,
            _ => Appearance::Dark,
        }
    }

    pub fn set_appearance(&mut self, appearance: Appearance) {
        self.set_str("topbar", "appearance", appearance_key(appearance));
    }

    #[must_use]
    pub(crate) fn appearance_scheduled(&self) -> bool {
        self.bool_or("topbar", "appearance-schedule", false)
    }

    #[must_use]
    pub fn appearance_at(&self, minute_of_day: Option<u16>) -> Appearance {
        match minute_of_day.filter(|_| self.appearance_scheduled()) {
            Some(minute) => glide_sdk::theme::Appearance::scheduled(minute).into(),
            None => self.appearance(),
        }
    }

    #[must_use]
    pub fn accent(&self) -> Accent {
        let key = self.get_str("topbar", "accent").unwrap_or("blue");
        Accent::ALL.iter().copied().find(|accent| accent_key(*accent) == key).unwrap_or_default()
    }

    pub fn set_accent(&mut self, accent: Accent) {
        self.set_str("topbar", "accent", accent_key(accent));
    }

    #[must_use]
    pub fn language(&self) -> Lang {
        self.language_choice().unwrap_or(self.system_language)
    }

    #[must_use]
    pub fn language_choice(&self) -> Option<Lang> {
        self.get_str("general", "language").and_then(Lang::from_key)
    }

    #[must_use]
    pub(crate) const fn system_language(&self) -> Lang {
        self.system_language
    }

    #[must_use]
    pub const fn with_system_language(mut self, lang: Lang) -> Self {
        self.system_language = lang;
        self
    }

    pub fn set_language(&mut self, lang: Option<Lang>) {
        match lang {
            Some(lang) => self.set_str("general", "language", lang.key()),
            None => self.remove("general", "language"),
        }
    }

    #[must_use]
    pub fn wallpaper(&self) -> Option<PathBuf> {
        self.get_str("general", "wallpaper").filter(|path| !path.is_empty()).map(PathBuf::from)
    }

    pub fn set_wallpaper(&mut self, path: Option<&Path>) {
        match path {
            Some(path) => self.set_str("general", "wallpaper", &path.to_string_lossy()),
            None => self.remove("general", "wallpaper"),
        }
    }

    #[must_use]
    pub fn wallpaper_fit(&self) -> WallpaperFit {
        self.get_str("general", "wallpaper-fit").and_then(WallpaperFit::from_key).unwrap_or_default()
    }

    pub fn set_wallpaper_fit(&mut self, fit: WallpaperFit) {
        self.set_str("general", "wallpaper-fit", fit.key());
    }

    #[must_use]
    pub fn animations_enabled(&self) -> bool {
        self.bool_or("animations", "enabled", true)
    }

    #[must_use]
    pub fn animation_speed(&self) -> f64 {
        self.get_float("animations", "speed").unwrap_or(1.0)
    }

    #[must_use]
    pub(crate) fn respect_system_motion(&self) -> bool {
        self.bool_or("animations", "respect-system", true)
    }

    #[must_use]
    pub fn night_light_schedule(&self) -> bool {
        self.bool_or("night-light", "schedule", false)
    }

    #[must_use]
    pub fn night_light_from(&self) -> String {
        self.str_or("night-light", "from", "20:00")
    }

    #[must_use]
    pub fn night_light_to(&self) -> String {
        self.str_or("night-light", "to", "07:00")
    }

    fn keyboard_or(&self, key: &str, fallback: fn(&glide_config::SystemKeyboard) -> &str) -> String {
        self.str_or("input", key, fallback(system_keyboard()))
    }

    #[must_use]
    pub fn xkb_layout(&self) -> String {
        self.keyboard_or("xkb-layout", |keyboard| &keyboard.layout)
    }

    #[must_use]
    pub fn xkb_variant(&self) -> String {
        self.keyboard_or("xkb-variant", |keyboard| &keyboard.variant)
    }

    #[must_use]
    pub fn xkb_options(&self) -> String {
        self.keyboard_or("xkb-options", |keyboard| &keyboard.options)
    }

    #[must_use]
    pub fn repeat_delay(&self) -> i64 {
        self.int_or("input", "repeat-delay", DEFAULT_REPEAT_DELAY)
    }

    #[must_use]
    pub fn repeat_rate(&self) -> i64 {
        self.int_or("input", "repeat-rate", DEFAULT_REPEAT_RATE)
    }

    #[must_use]
    pub fn tap_to_click(&self) -> bool {
        self.bool_or("input", "tap-to-click", true)
    }

    #[must_use]
    pub fn natural_scroll(&self) -> bool {
        self.bool_or("input", "natural-scroll", false)
    }

    #[must_use]
    pub fn accel_profile(&self) -> AccelProfile {
        match self.get_str("input", "accel-profile") {
            Some("flat") => AccelProfile::Flat,
            _ => AccelProfile::Adaptive,
        }
    }

    #[must_use]
    pub fn accel_speed(&self) -> f64 {
        self.get_float("input", "accel-speed").unwrap_or(0.0)
    }

    fn outputs(&self) -> Option<&ArrayOfTables> {
        self.doc.get("output")?.as_array_of_tables()
    }

    #[must_use]
    pub fn output_scale(&self, name: &str) -> Option<f64> {
        float(self.output_table(name)?.get("scale")?)
    }

    pub fn set_output_scale(&mut self, name: &str, scale: f64) {
        if self.error.is_some() || self.output_scale(name) == Some(scale) {
            return;
        }
        self.output_table_mut(name).insert("scale", value(scale));
        self.dirty = true;
    }

    fn output_table(&self, name: &str) -> Option<&Table> {
        self.outputs()?.iter().find(|table| table.get("name").and_then(Item::as_str) == Some(name))
    }

    #[must_use]
    pub fn output_mode(&self, name: &str) -> Option<String> {
        self.output_table(name)?.get("mode").and_then(Item::as_str).map(str::to_string)
    }

    #[must_use]
    pub fn output_position(&self, name: &str) -> Option<(i64, i64)> {
        let array = self.output_table(name)?.get("position")?.as_array()?;
        Some((array.get(0)?.as_integer()?, array.get(1)?.as_integer()?))
    }

    fn output_table_mut(&mut self, name: &str) -> &mut Table {
        let root = self.doc.as_table_mut();
        let item = root.entry("output").or_insert_with(|| Item::ArrayOfTables(ArrayOfTables::new()));
        if item.as_array_of_tables().is_none() {
            *item = Item::ArrayOfTables(ArrayOfTables::new());
        }
        let array = item.as_array_of_tables_mut().expect("the item was just made an array of tables");
        let index = array.iter().position(|table| table.get("name").and_then(Item::as_str) == Some(name));
        let index = index.unwrap_or_else(|| {
            let mut table = Table::new();
            table.insert("name", value(name));
            array.push(table);
            array.len() - 1
        });
        array.get_mut(index).expect("the index points inside the array")
    }

    pub(crate) fn tag_output(&mut self, name: &str, make: &str, model: &str) {
        let known = |text: &str| !text.trim().is_empty() && text.trim() != "Unknown";
        if self.error.is_some() || self.output_table(name).is_none() || !known(make) || !known(model) {
            return;
        }
        let table = self.output_table_mut(name);
        let mut changed = false;
        for (key, text) in [("make", make.trim()), ("model", model.trim())] {
            if table.get(key).and_then(Item::as_str) != Some(text) {
                table.insert(key, value(text));
                changed = true;
            }
        }
        self.dirty |= changed;
    }

    pub fn set_output_mode(&mut self, name: &str, mode: Option<&str>) {
        if self.error.is_some() || self.output_mode(name).as_deref() == mode {
            return;
        }
        let table = self.output_table_mut(name);
        if let Some(mode) = mode {
            table.insert("mode", value(mode));
        } else {
            table.remove("mode");
        }
        self.dirty = true;
    }

    pub fn set_output_positions(&mut self, positions: &[(String, i64, i64)]) {
        if self.error.is_some() {
            return;
        }
        for (name, x, y) in positions {
            if self.output_position(name) == Some((*x, *y)) {
                continue;
            }
            let array: toml_edit::Array = [*x, *y].into_iter().collect();
            self.output_table_mut(name).insert("position", value(array));
            self.dirty = true;
        }
    }

    #[must_use]
    pub(crate) fn output_snapshot(&self, name: &str) -> Option<Table> {
        self.output_table(name).cloned()
    }

    pub(crate) fn restore_output(&mut self, name: &str, snapshot: Option<&Table>) {
        let current = self.output_table(name).map(ToString::to_string);
        if self.error.is_some() || current == snapshot.map(ToString::to_string) {
            return;
        }
        if let Some(saved) = snapshot {
            *self.output_table_mut(name) = saved.clone();
        } else if let Some(array) = self.doc.get_mut("output").and_then(Item::as_array_of_tables_mut) {
            array.retain(|table| table.get("name").and_then(Item::as_str) != Some(name));
            if array.is_empty() {
                self.doc.as_table_mut().remove("output");
            }
        }
        self.dirty = true;
    }

    #[must_use]
    pub fn night_light_temperature(&self) -> i64 {
        self.int_or("night-light", "temperature", DEFAULT_NIGHT_LIGHT_KELVIN)
    }

    #[must_use]
    pub fn root_item(&self, key: &str) -> Option<&Item> {
        self.doc.get(key)
    }

    pub(crate) fn set_root_item(&mut self, key: &str, item: Item) {
        if self.error.is_some() {
            return;
        }
        if self.doc.get(key).map(ToString::to_string) == Some(item.to_string()) {
            return;
        }
        self.doc.as_table_mut().insert(key, item);
        self.dirty = true;
    }

    pub(crate) fn remove_root_item(&mut self, key: &str) {
        if self.error.is_some() {
            return;
        }
        if self.doc.as_table_mut().remove(key).is_some() {
            self.dirty = true;
        }
    }
}

pub(crate) fn write_atomic(target: &Path, contents: &[u8]) -> std::io::Result<()> {
    glide_config::write_atomic(target, contents, 0o644)
}
