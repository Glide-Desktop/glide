use serde::Deserialize;
use std::{
    fs,
    path::{Path, PathBuf},
};

use smithay::input::keyboard::{Keysym, ModifiersState, keysyms, xkb};

pub use crate::input::GestureConfig;
use crate::{GlideState, input::ResolvedKeybind};

pub use glide_theme::{Color, DecorationTheme, DockTheme, OverviewTheme, WallpaperFit};

pub trait OverviewThemeExt {
    fn options(&self) -> glide_layout::OverviewOptions;
}

impl OverviewThemeExt for OverviewTheme {
    fn options(&self) -> glide_layout::OverviewOptions {
        glide_layout::OverviewOptions {
            gap: self.gap,
            padding: self.padding,
            label_height: if self.label { self.label_height } else { 0 },
            label_offset: if self.label { crate::overview::label_offset(self.label_height) } else { 0 },
            max_scale: self.max_scale,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub general: GeneralConfig,
    pub workspaces: WorkspaceConfig,
    pub windows: WindowsConfig,
    pub keybinds: Vec<KeybindConfig>,
    pub decorations: DecorationTheme,
    pub overview: OverviewTheme,
    pub dock: DockConfig,
    pub topbar: TopbarConfig,
    #[serde(rename = "night-light")]
    pub night_light: NightLightConfig,
    pub input: InputConfig,
    pub gestures: GestureConfig,
    pub animations: AnimationConfig,
    pub notifications: NotificationsConfig,
    pub accessibility: AccessibilityConfig,
    pub displays: DisplaysConfig,
    #[serde(rename = "output")]
    pub outputs: Vec<OutputConfig>,
    #[serde(skip)]
    pub ignored_keys: Vec<String>,
    #[serde(skip)]
    pub load_error: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Arrangement {
    #[default]
    Extend,
    Mirror,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct DisplaysConfig {
    pub arrangement: Arrangement,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AccessibilityConfig {
    pub cursor_size: Option<u32>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
pub enum BannerPositionConfig {
    #[default]
    #[serde(rename = "top-right")]
    Right,
    #[serde(rename = "top-center")]
    Center,
    #[serde(rename = "top-left")]
    Left,
}

#[allow(clippy::struct_excessive_bools)] // Independent [notifications] switches mirrored from glide.toml
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NotificationsConfig {
    pub do_not_disturb: bool,
    pub schedule: bool,
    pub from: String,
    pub to: String,
    pub allow_critical: bool,
    pub banner_seconds: u32,
    pub position: BannerPositionConfig,
    pub muted_apps: Vec<String>,
    pub sounds: bool,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            do_not_disturb: false,
            schedule: false,
            from: "22:00".to_string(),
            to: "07:00".to_string(),
            allow_critical: true,
            banner_seconds: 0,
            position: BannerPositionConfig::default(),
            muted_apps: Vec::new(),
            sounds: true,
        }
    }
}

impl NotificationsConfig {
    #[must_use]
    pub fn policy(&self) -> crate::notifications::NotificationPolicy {
        use crate::notifications::BannerPosition;
        crate::notifications::NotificationPolicy {
            do_not_disturb: self.do_not_disturb,
            schedule: if self.schedule { parse_clock(&self.from).zip(parse_clock(&self.to)) } else { None },
            allow_critical: self.allow_critical,
            banner_timeout: (self.banner_seconds > 0)
                .then(|| std::time::Duration::from_secs(u64::from(self.banner_seconds.min(600)))),
            position: match self.position {
                BannerPositionConfig::Right => BannerPosition::Right,
                BannerPositionConfig::Center => BannerPosition::Center,
                BannerPositionConfig::Left => BannerPosition::Left,
            },
            muted: self.muted_apps.iter().map(|app| app.trim().to_string()).filter(|app| !app.is_empty()).collect(),
            sounds: self.sounds,
        }
    }
}

#[allow(clippy::struct_excessive_bools)] // Independent [topbar] switches, not a state machine
#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct TopbarConfig {
    #[serde(rename = "enabled")]
    _enabled: bool,
    pub appearance: glide_theme::Appearance,
    pub appearance_schedule: bool,
    pub accent: glide_theme::Accent,
    pub battery_low_pct: u8,
    pub max_volume_pct: u8,
    pub global_menu: bool,
    pub appmenu_registrar: bool,
    pub battery_percentage: bool,
    pub clock_date: bool,
    pub clock_24h: Option<bool>,
    pub tray: bool,
}

impl Default for TopbarConfig {
    fn default() -> Self {
        Self {
            _enabled: true,
            appearance: glide_theme::Appearance::Dark,
            appearance_schedule: false,
            accent: glide_theme::Accent::Blue,
            battery_low_pct: 20,
            max_volume_pct: 100,
            global_menu: true,
            appmenu_registrar: true,
            battery_percentage: true,
            clock_date: true,
            clock_24h: None,
            tray: true,
        }
    }
}

#[must_use]
pub const fn scheduled_appearance(minute_of_day: u16) -> glide_theme::Appearance {
    glide_theme::Appearance::scheduled(minute_of_day)
}

pub fn with_appearance(text: &str, appearance: glide_theme::Appearance) -> Result<String, String> {
    use toml_edit::{Item, Table, value};
    let mut doc: toml_edit::DocumentMut = text.parse().map_err(|err: toml_edit::TomlError| err.to_string())?;
    let item = doc.as_table_mut().entry("topbar").or_insert_with(|| Item::Table(Table::new()));
    let table = item.as_table_like_mut().ok_or_else(|| "[topbar] is not a table".to_string())?;
    let name = match appearance {
        glide_theme::Appearance::Light => "light",
        glide_theme::Appearance::Dark => "dark",
    };
    set_keeping_decor(table, "appearance", value(name));
    if table.get("appearance-schedule").is_some() {
        set_keeping_decor(table, "appearance-schedule", value(false));
    }
    Ok(doc.to_string())
}

fn set_keeping_decor(table: &mut dyn toml_edit::TableLike, key: &str, mut item: toml_edit::Item) {
    if let (Some(old), Some(new)) = (table.get(key).and_then(toml_edit::Item::as_value), item.as_value_mut()) {
        *new.decor_mut() = old.decor().clone();
    }
    table.insert(key, item);
}

pub fn write_appearance(path: &Path, appearance: glide_theme::Appearance) -> Result<(), String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err.to_string()),
    };
    let updated = with_appearance(&text, appearance)?;
    if updated == text {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    }
    write_atomic(path, updated.as_bytes()).map_err(|err| err.to_string())
}

pub fn persist_appearance(appearance: glide_theme::Appearance) {
    let Some(path) = Config::default_path() else {
        return;
    };
    let spawned = std::thread::Builder::new().name("glide-appearance".to_string()).spawn(move || {
        if let Err(err) = write_appearance(&path, appearance) {
            tracing::warn!(path = %path.display(), "could not save the appearance: {err}");
        }
    });
    if let Err(err) = spawned {
        tracing::warn!("the appearance writer did not start: {err}");
    }
}

impl TopbarConfig {
    #[must_use]
    pub fn effective_appearance(self, minute_of_day: Option<u16>) -> glide_theme::Appearance {
        match minute_of_day.filter(|_| self.appearance_schedule) {
            Some(minute) => scheduled_appearance(minute),
            None => self.appearance,
        }
    }

    #[must_use]
    pub fn theme(self) -> crate::topbar::TopbarTheme {
        let tokens = glide_theme::Tokens::new(self.appearance, self.accent);
        let mut theme = crate::topbar::TopbarTheme::from_tokens(&tokens);
        theme.battery_low_pct = self.battery_low_pct.min(100);
        theme.battery_percentage = self.battery_percentage;
        theme.clock_date = self.clock_date;
        theme.clock_24h = self.clock_24h;
        theme
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct NightLightConfig {
    pub schedule: bool,
    pub from: String,
    pub to: String,
    pub temperature: u32,
}

pub const NIGHT_LIGHT_DEFAULT_KELVIN: u32 = 4000;
pub const NIGHT_LIGHT_MIN_KELVIN: u32 = 1900;
pub const NIGHT_LIGHT_NEUTRAL_KELVIN: u32 = 6500;

impl Default for NightLightConfig {
    fn default() -> Self {
        Self {
            schedule: false,
            from: "20:00".to_string(),
            to: "07:00".to_string(),
            temperature: NIGHT_LIGHT_DEFAULT_KELVIN,
        }
    }
}

fn blackbody(kelvin: f64) -> (f64, f64) {
    let k = kelvin / 100.0;
    let green = 99.470_802_586_1 * k.ln() - 161.119_568_166_1;
    let blue = if k <= 19.0 { 0.0 } else { 138.517_731_223_1 * (k - 10.0).ln() - 305.044_792_730_7 };
    (green, blue)
}

#[must_use]
pub fn night_light_tint(kelvin: u32) -> (f64, f64, f64) {
    let kelvin = kelvin.clamp(NIGHT_LIGHT_MIN_KELVIN, NIGHT_LIGHT_NEUTRAL_KELVIN);
    let (green, blue) = blackbody(f64::from(kelvin));
    let (neutral_green, neutral_blue) = blackbody(f64::from(NIGHT_LIGHT_NEUTRAL_KELVIN));
    (1.0, (green / neutral_green).clamp(0.0, 1.0), (blue / neutral_blue).clamp(0.0, 1.0))
}

#[must_use]
pub fn night_light_ramp(size: usize, (red, green, blue): (f64, f64, f64)) -> Vec<u16> {
    let mut ramp = vec![0u16; size * 3];
    if size < 2 {
        return ramp;
    }
    for index in 0..size {
        let base = (index as f64 / (size as f64 - 1.0)) * f64::from(u16::MAX);
        {
            ramp[index] = (base * red).round() as u16;
            ramp[size + index] = (base * green).round() as u16;
            ramp[size * 2 + index] = (base * blue).round() as u16;
        }
    }
    ramp
}

impl NightLightConfig {
    #[must_use]
    pub fn window(&self) -> Option<(u16, u16)> {
        Some((parse_clock(&self.from)?, parse_clock(&self.to)?))
    }

    #[must_use]
    pub fn wants_on(&self, minute_of_day: u16) -> Option<bool> {
        if !self.schedule {
            return None;
        }
        let (from, to) = self.window()?;
        Some(if from <= to { (from..to).contains(&minute_of_day) } else { minute_of_day >= from || minute_of_day < to })
    }
}

fn parse_clock(text: &str) -> Option<u16> {
    let (hours, minutes) = text.trim().split_once(':')?;
    let hours: u16 = hours.parse().ok()?;
    let minutes: u16 = minutes.parse().ok()?;
    (hours < 24 && minutes < 60).then_some(hours * 60 + minutes)
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct AnimationConfig {
    pub enabled: bool,
    pub speed: f32,
    pub respect_system: bool,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self { enabled: true, speed: 1.0, respect_system: true }
    }
}

impl AnimationConfig {
    #[must_use]
    pub const fn motion_scale(self) -> f32 {
        if self.enabled { self.speed } else { 0.0 }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum LayoutScope {
    #[default]
    Global,
    Desktop,
    Application,
    Window,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NewWindowLayout {
    #[default]
    Default,
    Current,
}

#[allow(clippy::struct_excessive_bools)] // Independent [input] switches mirrored from glide.toml
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct InputConfig {
    pub xkb_layout: String,
    pub xkb_variant: String,
    pub layout_scope: LayoutScope,
    pub new_window_layout: NewWindowLayout,
    pub xkb_options: Option<String>,
    pub xkb_model: String,
    pub xkb_rules: String,
    pub repeat_delay: i32,
    pub repeat_rate: i32,
    pub numlock: bool,
    pub tap_to_click: bool,
    pub natural_scroll: bool,
    pub accel_profile: AccelProfileConfig,
    pub accel_speed: f64,
    pub left_handed: bool,
    pub middle_emulation: bool,
    pub disable_while_typing: bool,
    pub tap_and_drag: bool,
    pub scroll_method: ScrollMethodConfig,
    pub click_method: Option<ClickMethodConfig>,
    pub scroll_factor: f64,
    pub touchpad_accel_speed: Option<f64>,
    pub touchpad_accel_profile: Option<AccelProfileConfig>,
    pub touchpad_natural_scroll: Option<bool>,
    pub touchpad_scroll_factor: Option<f64>,
    pub smooth_wheel: bool,
    pub smooth_wheel_exclude: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum ScrollMethodConfig {
    #[default]
    TwoFinger,
    Edge,
    None,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ClickMethodConfig {
    Areas,
    Clickfinger,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointerSettings {
    pub accel_speed: f64,
    pub accel_profile: AccelProfileConfig,
    pub natural_scroll: bool,
    pub scroll_factor: f64,
}

pub const SCROLL_FACTOR_RANGE: std::ops::RangeInclusive<f64> = 0.1..=5.0;

#[must_use]
pub fn is_touchpad<D: smithay::backend::input::Device>(device: &D) -> bool {
    device.has_capability(smithay::backend::input::DeviceCapability::Gesture)
}

fn clamp_scroll_factor(factor: f64) -> f64 {
    if factor.is_finite() { factor.clamp(*SCROLL_FACTOR_RANGE.start(), *SCROLL_FACTOR_RANGE.end()) } else { 1.0 }
}

impl Default for InputConfig {
    fn default() -> Self {
        let keyboard = if cfg!(test) {
            glide_config::SystemKeyboard::for_locale("uk_UA.UTF-8")
        } else {
            glide_config::SystemKeyboard::detect()
        };
        Self {
            xkb_layout: keyboard.layout,
            xkb_variant: keyboard.variant,
            layout_scope: LayoutScope::Global,
            new_window_layout: NewWindowLayout::Default,
            xkb_options: (!keyboard.options.is_empty()).then_some(keyboard.options),
            xkb_model: keyboard.model,
            xkb_rules: String::new(),
            repeat_delay: 200,
            repeat_rate: 25,
            numlock: true,
            tap_to_click: true,
            natural_scroll: false,
            accel_profile: AccelProfileConfig::default(),
            accel_speed: 0.0,
            left_handed: false,
            middle_emulation: false,
            disable_while_typing: true,
            tap_and_drag: true,
            scroll_method: ScrollMethodConfig::default(),
            click_method: None,
            scroll_factor: 1.0,
            touchpad_accel_speed: None,
            touchpad_accel_profile: None,
            touchpad_natural_scroll: None,
            touchpad_scroll_factor: None,
            smooth_wheel: true,
            smooth_wheel_exclude: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AccelProfileConfig {
    #[default]
    Adaptive,
    Flat,
}

impl InputConfig {
    #[must_use]
    pub fn xkb(&self) -> smithay::input::keyboard::XkbConfig<'_> {
        smithay::input::keyboard::XkbConfig {
            rules: &self.xkb_rules,
            model: &self.xkb_model,
            layout: &self.xkb_layout,
            variant: &self.xkb_variant,
            options: self.xkb_options.clone(),
        }
    }

    #[must_use]
    pub fn pointer_settings(&self, touchpad: bool) -> PointerSettings {
        let shared = PointerSettings {
            accel_speed: self.accel_speed,
            accel_profile: self.accel_profile,
            natural_scroll: self.natural_scroll,
            scroll_factor: self.scroll_factor,
        };
        let settings = if touchpad {
            PointerSettings {
                accel_speed: self.touchpad_accel_speed.unwrap_or(shared.accel_speed),
                accel_profile: self.touchpad_accel_profile.unwrap_or(shared.accel_profile),
                natural_scroll: self.touchpad_natural_scroll.unwrap_or(shared.natural_scroll),
                scroll_factor: self.touchpad_scroll_factor.unwrap_or(shared.scroll_factor),
            }
        } else {
            shared
        };
        PointerSettings {
            accel_speed: if settings.accel_speed.is_finite() { settings.accel_speed.clamp(-1.0, 1.0) } else { 0.0 },
            scroll_factor: clamp_scroll_factor(settings.scroll_factor),
            ..settings
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Default)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct OutputConfig {
    pub name: String,
    pub make: Option<String>,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub position: Option<[i32; 2]>,
    pub scale: Option<f64>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OutputIdentity {
    pub name: String,
    pub make: String,
    pub model: String,
    pub serial: String,
}

impl OutputIdentity {
    #[must_use]
    pub fn of(output: &smithay::output::Output) -> Self {
        let properties = output.physical_properties();
        Self { name: output.name(), make: properties.make, model: properties.model, serial: properties.serial_number }
    }
}

fn given(want: Option<&str>) -> Option<&str> {
    want.map(str::trim).filter(|want| !want.is_empty())
}

impl OutputConfig {
    #[must_use]
    pub fn score(&self, identity: &OutputIdentity) -> Option<u8> {
        let wanted = [
            (self.make.as_deref(), identity.make.as_str()),
            (self.model.as_deref(), identity.model.as_str()),
            (self.serial.as_deref(), identity.serial.as_str()),
        ];
        let named = !self.name.is_empty() && self.name == identity.name;
        if wanted.iter().all(|(want, _)| given(*want).is_none()) {
            return named.then_some(1);
        }
        let same = wanted.iter().all(|(want, have)| given(*want).is_none_or(|want| want == have.trim()));
        same.then_some(if named { 3 } else { 2 })
    }
}

#[must_use]
pub fn output_config<'a>(configs: &'a [OutputConfig], identity: &OutputIdentity) -> Option<&'a OutputConfig> {
    let mut best: Option<(u8, &OutputConfig)> = None;
    for config in configs {
        if let Some(score) = config.score(identity)
            && best.is_none_or(|(best, _)| score > best)
        {
            best = Some((score, config));
        }
    }
    best.map(|(_, config)| config)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeSpec {
    pub width: i32,
    pub height: i32,
    pub refresh_mhz: Option<i32>,
}

impl ModeSpec {
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        let (size, refresh) = match text.split_once('@') {
            Some((size, refresh)) => (size, Some(refresh.trim())),
            None => (text, None),
        };
        let (width, height) = size.trim().split_once(['x', 'X'])?;
        let width: i32 = width.trim().parse().ok()?;
        let height: i32 = height.trim().parse().ok()?;
        if width <= 0 || height <= 0 {
            return None;
        }
        let refresh_mhz = match refresh {
            Some(hz) => {
                let hz: f64 = hz.trim_end_matches("Hz").trim().parse().ok()?;
                if !(hz.is_finite() && hz > 0.0 && hz < 1000.0) {
                    return None;
                }
                Some((hz * 1000.0).round() as i32)
            }
            None => None,
        };
        Some(Self { width, height, refresh_mhz })
    }

    #[must_use]
    pub fn pick(self, modes: &[(i32, i32, i32, bool)]) -> Option<usize> {
        modes
            .iter()
            .enumerate()
            .filter(|(_, (w, h, _, _))| *w == self.width && *h == self.height)
            .min_by_key(|(_, (_, _, refresh, preferred))| match self.refresh_mhz {
                Some(wanted) => (i64::from((refresh - wanted).abs()), i64::from(!*preferred)),
                None => (i64::from(!*preferred), -i64::from(*refresh)),
            })
            .filter(|(_, (_, _, refresh, _))| self.refresh_mhz.is_none_or(|wanted| (refresh - wanted).abs() <= 500))
            .map(|(index, _)| index)
    }
}

impl OutputConfig {
    #[must_use]
    pub fn mode_spec(&self) -> Option<ModeSpec> {
        self.mode.as_deref().and_then(ModeSpec::parse)
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, rename_all = "kebab-case")]
#[derive(Default)]
pub struct DockConfig {
    #[serde(flatten)]
    pub theme: DockTheme,
    pub autohide: DockAutohide,
    pub items: Vec<DockItemConfig>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum DockAutohide {
    #[default]
    Smart,
    Always,
    Never,
}

#[derive(Debug, Clone, Default, serde::Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DockPins {
    pub pinned: Vec<PinnedApp>,
}

#[derive(Debug, Clone, serde::Serialize, Deserialize)]
pub struct PinnedApp {
    pub app_id: String,
    pub name: String,
    #[serde(default)]
    pub command: String,
}

const PINS_FILE: &str = "pins.toml";
pub const CONFIG_NOTICE_ID: u32 = u32::MAX - 4;
const CONFIG_NOTICE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStamp {
    path: PathBuf,
    modified: Option<std::time::SystemTime>,
    len: Option<u64>,
}

impl FileStamp {
    #[must_use]
    pub fn of(path: &Path) -> Self {
        let meta = fs::metadata(path).ok();
        Self {
            path: path.to_path_buf(),
            modified: meta.as_ref().and_then(|meta| meta.modified().ok()),
            len: meta.as_ref().map(fs::Metadata::len),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Applied<T>(T);

type Keymap = (String, String, String, String, Option<String>);

#[derive(Debug, Default)]
pub struct AppliedConfig {
    wallpaper: Option<Applied<Option<FileStamp>>>,
    icon_theme: Option<Applied<Option<String>>>,
    pins: Option<(Option<FileStamp>, Vec<DockItemConfig>)>,
    keymap: Option<Applied<Keymap>>,
    numlock: Option<Applied<bool>>,
}

fn replace_if_changed<T: PartialEq>(slot: &mut Option<Applied<T>>, value: T) -> bool {
    let value = Some(Applied(value));
    if *slot == value {
        return false;
    }
    *slot = value;
    true
}

impl AppliedConfig {
    fn wallpaper_changed(&mut self, wallpaper: Option<&Path>) -> bool {
        replace_if_changed(&mut self.wallpaper, wallpaper.map(FileStamp::of))
    }

    fn icon_theme_changed(&mut self, theme: Option<&str>) -> bool {
        replace_if_changed(&mut self.icon_theme, theme.map(str::to_owned))
    }

    pub fn keymap_changed(&mut self, input: &InputConfig) -> bool {
        let keymap = (
            input.xkb_rules.clone(),
            input.xkb_model.clone(),
            input.xkb_layout.clone(),
            input.xkb_variant.clone(),
            input.xkb_options.clone(),
        );
        replace_if_changed(&mut self.keymap, keymap)
    }

    pub fn numlock_changed(&mut self, numlock: bool) -> bool {
        replace_if_changed(&mut self.numlock, numlock)
    }

    fn pins(&mut self) -> Vec<DockItemConfig> {
        let stamp = glide_config::xdg_path("glide", PINS_FILE).map(|path| FileStamp::of(&path));
        if let Some((cached_stamp, pins)) = &self.pins
            && stamp.as_ref().is_some_and(|s| s.modified.is_some())
            && *cached_stamp == stamp
        {
            return pins.clone();
        }
        let pins = load_or_seed_dock_pins();
        let stamp = glide_config::xdg_path("glide", PINS_FILE).map(|path| FileStamp::of(&path));
        self.pins = Some((stamp, pins.clone()));
        pins
    }
}

#[must_use]
pub fn load_dock_pins() -> Option<Vec<DockItemConfig>> {
    let path = glide_config::xdg_path("glide", PINS_FILE)?;
    let text = fs::read_to_string(&path).ok()?;
    match toml::from_str::<DockPins>(&text) {
        Ok(pins) => Some(
            pins.pinned
                .into_iter()
                .map(|pin| DockItemConfig {
                    name: pin.name,
                    command: pin.command,
                    icon: String::new(),
                    app_id: Some(pin.app_id),
                    color_from: None,
                    color_to: None,
                    separator_before: false,
                })
                .collect(),
        ),
        Err(err) => {
            tracing::warn!(path = %path.display(), "ignoring malformed dock pins: {err}");
            Some(Vec::new())
        }
    }
}

#[must_use]
pub fn load_or_seed_dock_pins() -> Vec<DockItemConfig> {
    if let Some(pins) = load_dock_pins() {
        return pins;
    }
    let seed = DockItemConfig::installed_defaults();
    save_pins(seed.iter().map(|item| PinnedApp {
        app_id: item.app_id.clone().unwrap_or_default(),
        name: item.name.clone(),
        command: item.command.clone(),
    }));
    seed
}

pub fn save_dock_pins(items: &[crate::dock::DockItem]) {
    save_pins(items.iter().map(|item| PinnedApp {
        app_id: item.app_id.clone(),
        name: item.name.clone(),
        command: item.command.clone(),
    }));
}

fn save_pins(pinned: impl Iterator<Item = PinnedApp>) {
    let Some(path) = glide_config::xdg_path("glide", PINS_FILE) else {
        return;
    };
    let pins = DockPins { pinned: pinned.collect() };

    let Ok(text) = toml::to_string_pretty(&pins) else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Err(err) = write_atomic(&path, text.as_bytes()) {
        tracing::error!(path = %path.display(), "failed to save dock pins: {err}");
    }
}

fn temp_sibling(target: &Path) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut name = std::ffi::OsString::from(".");
    name.push(target.file_name().unwrap_or_default());
    name.push(format!(".{}.{serial}.glide.tmp", std::process::id()));
    target.with_file_name(name)
}

fn resolve_symlinks(target: &Path) -> PathBuf {
    let mut resolved = target.to_path_buf();
    for _ in 0..40 {
        let Ok(link) = fs::read_link(&resolved) else {
            break;
        };
        resolved = match resolved.parent() {
            Some(parent) if link.is_relative() => parent.join(link),
            _ => link,
        };
    }
    resolved
}

pub fn write_atomic(target: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;

    let target = &resolve_symlinks(target);
    let tmp = temp_sibling(target);
    let written = (|| {
        let mut file = fs::OpenOptions::new().write(true).create_new(true).open(&tmp)?;
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(&tmp, target)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written?;
    if let Some(dir) = target.parent()
        && let Ok(dir) = fs::File::open(if dir.as_os_str().is_empty() { Path::new(".") } else { dir })
    {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[must_use]
pub fn last_good_path(config: &Path) -> PathBuf {
    let mut name = config.file_name().unwrap_or_default().to_os_string();
    name.push(".last-good");
    config.with_file_name(name)
}

fn remember_last_good(config: &Path, content: &str) {
    let target = last_good_path(config);
    if fs::read_to_string(&target).is_ok_and(|existing| existing == content) {
        return;
    }
    if let Err(err) = write_atomic(&target, content.as_bytes()) {
        tracing::warn!(path = %target.display(), "failed to save last known good config: {err}");
    }
}

#[must_use]
pub fn pin_identity_for_app_id(app_id: &str) -> Option<(String, String)> {
    crate::icon::find_application(app_id).map(|app| (app.name, app.command))
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
                let app = crate::icon::find_application(item.app_id.as_deref()?)?;
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

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct GeneralConfig {
    pub background: Color,
    pub autostart: Vec<String>,
    pub xdg_autostart: bool,
    pub polkit_agent: bool,
    pub pause_media_on_disconnect: bool,
    pub wallpaper: Option<PathBuf>,
    pub wallpaper_fit: WallpaperFit,
    pub font_family: Option<String>,
    pub cursor_theme: Option<String>,
    pub blank_after_seconds: u64,
    pub lock_after_seconds: i64,
    pub suspend_after_seconds: u64,
    pub suspend_on_battery_after_seconds: u64,
    pub terminal: Option<String>,
    pub language: Option<crate::i18n::Lang>,
}

impl GeneralConfig {
    #[must_use]
    pub fn idle_policy(&self) -> crate::idle::Policy {
        let after = |seconds: u64| (seconds > 0).then(|| std::time::Duration::from_secs(seconds));
        crate::idle::Policy {
            blank: after(self.blank_after_seconds),
            lock: u64::try_from(self.lock_after_seconds).ok().map(std::time::Duration::from_secs),
            suspend: after(self.suspend_after_seconds),
            suspend_on_battery: after(self.suspend_on_battery_after_seconds),
        }
    }
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            background: Color::from_hex("#222c3cff").unwrap(),
            autostart: vec![],
            xdg_autostart: true,
            polkit_agent: true,
            pause_media_on_disconnect: true,
            wallpaper: None,
            wallpaper_fit: WallpaperFit::default(),
            font_family: None,
            cursor_theme: None,
            blank_after_seconds: 600,
            lock_after_seconds: 0,
            suspend_after_seconds: 0,
            suspend_on_battery_after_seconds: 0,
            terminal: None,
            language: None,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Default)]
#[serde(default, deny_unknown_fields)]
pub struct WorkspaceConfig {
    #[serde(rename = "new-window")]
    pub new_window: NewWindowPolicyConfig,
    pub fixed: usize,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields, rename_all = "kebab-case")]
pub struct WindowsConfig {
    pub edge_snapping: bool,
}

impl Default for WindowsConfig {
    fn default() -> Self {
        Self { edge_snapping: true }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum NewWindowPolicyConfig {
    #[default]
    NewWorkspace,
    ActiveWorkspace,
}

impl From<NewWindowPolicyConfig> for glide_layout::NewWindowPolicy {
    fn from(cfg: NewWindowPolicyConfig) -> Self {
        match cfg {
            NewWindowPolicyConfig::NewWorkspace => glide_layout::NewWindowPolicy::NewWorkspace,
            NewWindowPolicyConfig::ActiveWorkspace => glide_layout::NewWindowPolicy::ActiveWorkspace,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModifierConfig {
    Super,
    Logo,
    Shift,
    Ctrl,
    Control,
    Alt,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KeybindConfig {
    #[serde(default)]
    pub mods: Vec<ModifierConfig>,
    pub key: String,
    pub action: KeyActionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum KeyActionConfig {
    Simple(String),
    Complex(ParameterizedAction),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct ParameterizedAction {
    #[serde(rename = "switch-workspace")]
    pub switch_workspace: Option<usize>,
    #[serde(rename = "move-window-to-workspace")]
    pub move_window_to_workspace: Option<usize>,
    pub exec: Option<String>,
    pub vt: Option<i32>,
}

impl Config {
    #[must_use]
    pub fn default_keybinds() -> Vec<KeybindConfig> {
        use ModifierConfig::{Alt, Ctrl, Shift, Super};
        let bind = |mods: &[ModifierConfig], key: &str, action: KeyActionConfig| KeybindConfig {
            mods: mods.to_vec(),
            key: key.to_string(),
            action,
        };
        let simple = |mods: &[ModifierConfig], key: &str, action: &str| {
            bind(mods, key, KeyActionConfig::Simple(action.to_string()))
        };
        let exec = |mods: &[ModifierConfig], key: &str, command: &str| {
            let action = ParameterizedAction { exec: Some(command.to_string()), ..ParameterizedAction::default() };
            bind(mods, key, KeyActionConfig::Complex(action))
        };
        let mut keybinds = vec![
            simple(&[Super], "q", "close-app"),
            simple(&[Super, Shift], "q", "logout"),
            simple(&[Super], "Tab", "next-workspace"),
            simple(&[Super, Shift], "Tab", "prev-workspace"),
            simple(&[Super], "f", "maximize"),
            simple(&[Super, Shift], "f", "fullscreen"),
            simple(&[Super], "m", "minimize"),
            simple(&[Super], "o", "toggle-overview"),
            simple(&[Alt], "Tab", "switch-windows"),
            simple(&[Alt, Shift], "Tab", "switch-windows-backward"),
            simple(&[Alt], "grave", "switch-app-windows"),
            simple(&[Alt, Shift], "grave", "switch-app-windows-backward"),
            simple(&[Alt], "F4", "close-window"),
            simple(&[Alt], "space", "open-window-menu"),
            simple(&[], "Print", "screenshot"),
            simple(&[Shift], "Print", "screenshot-screen"),
            simple(&[Alt], "Print", "screenshot-window"),
            simple(&[Super], "a", "toggle-launcher"),
            exec(
                &[],
                "XF86AudioRaiseVolume",
                "wpctl set-volume -l 1.5 @DEFAULT_AUDIO_SINK@ 5%+ || pactl set-sink-volume @DEFAULT_SINK@ +5%",
            ),
            exec(
                &[],
                "XF86AudioLowerVolume",
                "wpctl set-volume @DEFAULT_AUDIO_SINK@ 5%- || pactl set-sink-volume @DEFAULT_SINK@ -5%",
            ),
            exec(
                &[],
                "XF86AudioMute",
                "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle || pactl set-sink-mute @DEFAULT_SINK@ toggle",
            ),
            exec(
                &[],
                "XF86AudioMicMute",
                "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle || pactl set-source-mute @DEFAULT_SOURCE@ toggle",
            ),
            exec(&[], "XF86MonBrightnessUp", "brightnessctl set +5%"),
            exec(&[], "XF86MonBrightnessDown", "brightnessctl set 5%-"),
            exec(&[], "XF86KbdBrightnessUp", "brightnessctl --device='*::kbd_backlight' set +33%"),
            exec(&[], "XF86KbdBrightnessDown", "brightnessctl --device='*::kbd_backlight' set 33%-"),
            simple(&[], "XF86Search", "toggle-launcher"),
            simple(&[], "XF86Calculator", "toggle-launcher"),
            simple(&[], "XF86AudioPlay", "media-play-pause"),
            simple(&[], "XF86AudioPause", "media-pause"),
            simple(&[], "XF86AudioStop", "media-stop"),
            simple(&[], "XF86AudioNext", "media-next"),
            simple(&[], "XF86AudioPrev", "media-previous"),
            simple(&[], "XF86AudioForward", "media-forward"),
            simple(&[], "XF86AudioRewind", "media-rewind"),
            simple(&[Super], "l", "lock"),
            simple(&[Ctrl, Alt], "Delete", "power-dialog"),
            simple(&[], "XF86PowerOff", "power-dialog"),
            simple(&[Super], "d", "show-desktop"),
            simple(&[Super], "v", "toggle-notification-center"),
            simple(&[Super], "s", "toggle-quick-settings"),
            simple(&[Super, Shift], "Page_Down", "move-window-to-next-workspace"),
            simple(&[Super, Shift], "Page_Up", "move-window-to-prev-workspace"),
            simple(&[Ctrl, Alt], "t", "terminal"),
            simple(&[Super], "Left", "tile-left"),
            simple(&[Super], "Right", "tile-right"),
            simple(&[Super], "Up", "tile-maximize"),
            simple(&[Super], "Down", "tile-restore"),
            simple(&[Super, Shift], "Left", "move-window-to-monitor-left"),
            simple(&[Super, Shift], "Right", "move-window-to-monitor-right"),
            simple(&[Ctrl], "F2", "focus-top-bar"),
            simple(&[Ctrl], "F3", "focus-dock"),
            simple(&[Alt], "F2", "run-command"),
        ];
        for i in 1..=9 {
            let key = i.to_string();
            let switch = ParameterizedAction { switch_workspace: Some(i), ..ParameterizedAction::default() };
            let send = ParameterizedAction { move_window_to_workspace: Some(i), ..ParameterizedAction::default() };
            keybinds.push(bind(&[Super], &key, KeyActionConfig::Complex(switch)));
            keybinds.push(bind(&[Super, Shift], &key, KeyActionConfig::Complex(send)));
        }
        keybinds
    }

    #[must_use]
    pub fn default_path() -> Option<PathBuf> {
        glide_config::xdg_path("glide", "glide.toml")
    }

    #[must_use]
    pub fn load_or_default() -> Self {
        let Some(path) = Self::default_path() else {
            tracing::info!("could not determine XDG_CONFIG_HOME or HOME; using default config");
            return Self::with_defaults();
        };

        Self::load_from_path(&path)
    }

    #[must_use]
    pub fn with_defaults() -> Self {
        Self { keybinds: Self::default_keybinds(), ..Self::default() }
    }

    pub fn try_load() -> Result<Self, String> {
        match Self::default_path() {
            Some(path) => Self::try_load_from_path(&path),
            None => Ok(Self::with_defaults()),
        }
    }

    pub fn try_load_from_path(path: &std::path::Path) -> Result<Self, String> {
        let content = match fs::read_to_string(path) {
            Ok(content) => content,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::with_defaults());
            }
            Err(err) => return Err(format!("{}: {err}", path.display())),
        };
        let config = Self::parse(&content).map_err(|err| format!("{}: {err}", path.display()))?;
        remember_last_good(path, &content);
        Ok(config)
    }

    fn parse(content: &str) -> Result<Self, toml::de::Error> {
        let repaired = crate::config_repair::parse::<Config>(content)?;
        let mut config = repaired.value;
        if !repaired.ignored.is_empty() {
            tracing::warn!(keys = ?repaired.ignored, "glide.toml has settings this version does not know; ignoring them");
        }
        config.ignored_keys = repaired.ignored;
        if config.keybinds.is_empty() {
            config.keybinds = Self::default_keybinds();
        }
        Ok(config)
    }

    #[must_use]
    pub fn load_from_path(path: &std::path::Path) -> Self {
        if !path.exists() {
            tracing::info!(path = %path.display(), "config file not found; using default config");
            return Self::with_defaults();
        }

        let failure = match fs::read_to_string(path) {
            Ok(content) => match Self::parse(&content) {
                Ok(config) => {
                    tracing::info!(path = %path.display(), "config loaded successfully");
                    remember_last_good(path, &content);
                    return config;
                }
                Err(err) => format!("failed to parse: {err}"),
            },
            Err(err) => format!("failed to read: {err}"),
        };

        let fallback = last_good_path(path);
        let recovered = fs::read_to_string(&fallback).ok().and_then(|content| Self::parse(&content).ok());
        let config = if let Some(config) = recovered {
            tracing::error!(
                path = %path.display(),
                fallback = %fallback.display(),
                "config {failure}; using the last known good config"
            );
            config
        } else {
            tracing::error!(
                path = %path.display(),
                "config {failure}; no usable last known good config, using defaults"
            );
            Self::with_defaults()
        };
        Self { load_error: Some(failure), ..config }
    }

    fn announce_problems(&self, state: &mut GlideState) {
        use crate::i18n::{Key, t};
        let known = std::mem::replace(&mut state.session.announced_config_keys, self.ignored_keys.clone());
        let (summary, body) = if let Some(error) = &self.load_error {
            (t(Key::ConfigBroken), error.clone())
        } else if self.ignored_keys.is_empty() || known == self.ignored_keys {
            return;
        } else {
            (t(Key::ConfigIgnoredKeys), self.ignored_keys.join(", "))
        };
        let notice = glide_notify::Notification {
            id: CONFIG_NOTICE_ID,
            app_name: "Glide".to_string(),
            summary: summary.to_string(),
            body,
            timeout: Some(CONFIG_NOTICE_TIMEOUT),
            ..glide_notify::Notification::default()
        };
        state.notifications.push(&mut state.dock.icons, notice, true);
    }

    #[must_use]
    pub fn resolve_keybinds(&self) -> Vec<ResolvedKeybind> {
        self.keybinds
            .iter()
            .filter_map(|entry| {
                let mut modifiers = ModifiersState::default();
                for m in &entry.mods {
                    match m {
                        ModifierConfig::Super | ModifierConfig::Logo => modifiers.logo = true,
                        ModifierConfig::Shift => modifiers.shift = true,
                        ModifierConfig::Ctrl | ModifierConfig::Control => modifiers.ctrl = true,
                        ModifierConfig::Alt => modifiers.alt = true,
                    }
                }
                let keysym = parse_keysym(&entry.key);
                let action = parse_action(&entry.action);
                Some(ResolvedKeybind { modifiers, keysym: keysym?, action: action? })
            })
            .collect()
    }

    fn apply_window_policy(&self, state: &mut GlideState) {
        state.layout.set_policy(self.workspaces.new_window.into());
        let fixed = (self.workspaces.fixed > 0).then_some(self.workspaces.fixed);
        if state.layout.set_fixed_workspaces(fixed) {
            state.reconcile_space();
            state.sync_ext_workspaces();
        }
        state.edge_snapping = self.windows.edge_snapping;
        state.gestures.config = self.gestures;
    }

    pub fn apply_to_state(&self, state: &mut GlideState) {
        let appearance = self.topbar.effective_appearance(crate::topbar::local_minute_of_day());
        if appearance != self.topbar.appearance {
            let mut scheduled = self.clone();
            scheduled.topbar.appearance = appearance;
            scheduled.apply_to_state(state);
            return;
        }
        state.session.appearance_schedule = self.topbar.appearance_schedule;
        state.session.headset.pause_on_unplug = self.general.pause_media_on_disconnect;
        state.motion = crate::state::MotionPolicy {
            configured_scale: self.animations.motion_scale(),
            respect_system: self.animations.respect_system,
        };
        state.apply_motion_scale();
        state.set_night_light_config(self.night_light.clone());
        self.apply_window_policy(state);
        state.keybinds = self.resolve_keybinds();
        let palette = glide_theme::Tokens::new(self.topbar.appearance, self.topbar.accent);
        state.decoration_theme = self.decorations.clone();
        state.decoration_theme.repaint(&palette);
        state.overview_theme = self.overview.clone();
        state.overview_theme.repaint(&palette);
        let mut dock = self.dock.clone();
        dock.theme.repaint(&palette);
        for pin in state.applied_config.pins() {
            let already = dock.items.iter().any(|item| {
                item.app_id
                    .as_deref()
                    .is_some_and(|id| pin.app_id.as_deref().is_some_and(|p| id.eq_ignore_ascii_case(p)))
            });
            if !already {
                dock.items.push(pin);
            }
        }
        state.dock.reconfigure(&dock);
        state.refresh_dock();
        state.topbar.reconfigure(self.topbar.theme());
        state.shell_palette = (self.topbar.appearance, self.topbar.accent);
        crate::system_info::sync_desktop_appearance(crate::system_info::DesktopAppearance {
            appearance: self.topbar.appearance,
            accent: self.topbar.accent,
            icon_theme: self.dock.theme.icon_theme.clone(),
            cursor_theme: self.general.cursor_theme.clone(),
            cursor_size: self.accessibility.cursor_size,
            font_family: self.general.font_family.clone(),
            wallpaper: self.general.wallpaper.clone(),
            toolkits: glide_config::toolkits::Support::detect(),
        });
        state.max_volume_pct = self.topbar.max_volume_pct.clamp(100, 150);
        state.app_menus.registrar = self.topbar.appmenu_registrar;
        state.set_global_menu_enabled(self.topbar.global_menu);
        state.configure_tray(&self.topbar, self.dock.theme.icon_theme.clone());
        state.notifications.set_policy(self.notifications.policy());
        crate::cursor::set_configured_size(self.accessibility.cursor_size);
        if crate::cursor::set_configured_theme(self.general.cursor_theme.as_deref()) {
            state.reload_cursor_theme();
        }
        self.announce_problems(state);
        state.refresh_topbar();
        state.launcher_theme = crate::launcher::LauncherTheme::from_dock(&dock.theme, &palette);
        state.notifications.set_theme(&crate::notifications::NotificationTheme::from_tokens(&palette));
        if state.applied_config.icon_theme_changed(self.dock.theme.icon_theme.as_deref()) {
            state.launcher_icons = crate::icon::IconLoader::new(self.dock.theme.icon_theme.clone());
        }
        crate::i18n::set_language(self.general.language.unwrap_or_else(crate::i18n::Lang::from_env));
        state.background_color = self.general.background;
        state.wallpaper_fit = self.general.wallpaper_fit;
        state.font_family.clone_from(&self.general.font_family);
        state.idle.policy = self.general.idle_policy();
        glide_config::terminal::set_configured(self.general.terminal.as_deref());
        state.input_config.clone_from(&self.input);
        state.output_configs.clone_from(&self.outputs);
        state.session.arrangement = self.displays.arrangement;
        state.apply_device_configs();
        state.refresh_painter_font();

        if state.applied_config.wallpaper_changed(self.general.wallpaper.as_deref()) {
            let wallpaper_bytes = self.general.wallpaper.as_ref().and_then(|path| {
                fs::read(path)
                    .inspect_err(|err| {
                        tracing::warn!(path = %path.display(), "failed to read wallpaper: {err}");
                    })
                    .ok()
            });
            glide_paint::init_wallpaper(wallpaper_bytes.as_deref());
        }

        if let Ok(height) = i32::try_from(self.decorations.height) {
            let insets = glide_layout::Insets { top: height, ..glide_layout::Insets::NONE };
            for id in server_decorated(&state.layout, state.window_map.keys().copied()) {
                state.layout.set_window_insets(id, insets);
            }
            state.reconcile_space();
        }

        tracing::info!(
            policy = ?self.workspaces.new_window,
            keybind_count = state.keybinds.len(),
            "applied configuration"
        );
    }
}

#[must_use]
pub fn server_decorated(
    layout: &glide_layout::Layout, ids: impl Iterator<Item = glide_layout::WindowId>,
) -> Vec<glide_layout::WindowId> {
    ids.filter(|id| layout.window_state(*id).is_some_and(|window| window.insets.top > 0)).collect()
}

fn parse_keysym(s: &str) -> Option<Keysym> {
    if let Ok(digit @ 0..=9) = s.parse::<u32>() {
        return Some(Keysym::from(keysyms::KEY_0 + digit));
    }
    let sym_raw = xkb::keysym_from_name(s, xkb::KEYSYM_NO_FLAGS);
    if sym_raw.raw() == keysyms::KEY_NoSymbol {
        tracing::warn!(key = s, "unrecognized key name in config");
        None
    } else {
        Some(sym_raw)
    }
}

fn parse_action(cfg: &KeyActionConfig) -> Option<crate::input::KeyAction> {
    use crate::input::KeyAction;
    match cfg {
        KeyActionConfig::Simple(cmd) => Some(match cmd.to_lowercase().as_str() {
            "quit" => KeyAction::Quit,
            "next-workspace" => KeyAction::NextWorkspace,
            "prev-workspace" => KeyAction::PrevWorkspace,
            "maximize" => KeyAction::Maximize,
            "fullscreen" => KeyAction::Fullscreen,
            "minimize" => KeyAction::Minimize,
            "close-window" => KeyAction::CloseFocused,
            "open-window-menu" => KeyAction::OpenWindowMenu,
            "toggle-overview" => KeyAction::ToggleOverview,
            "screenshot" => KeyAction::Screenshot,
            "screenshot-screen" => KeyAction::ScreenshotScreen,
            "screenshot-window" => KeyAction::ScreenshotWindow,
            "toggle-launcher" => KeyAction::ToggleLauncher,
            "switch-windows" => KeyAction::SwitchWindows(false),
            "switch-windows-backward" => KeyAction::SwitchWindows(true),
            "switch-app-windows" => KeyAction::SwitchAppWindows(false),
            "switch-app-windows-backward" => KeyAction::SwitchAppWindows(true),
            "close-app" => KeyAction::CloseApp,
            "lock" => KeyAction::Lock,
            "logout" => KeyAction::Logout,
            "power-dialog" => KeyAction::PowerDialog,
            "terminal" => KeyAction::Terminal,
            "force-quit" => KeyAction::ForceQuit,
            "fill-password" => KeyAction::FillPassword,
            "screen-mirroring" => KeyAction::ScreenMirroring,
            "tile-left" => KeyAction::Tile(crate::tiling::Side::Left),
            "tile-right" => KeyAction::Tile(crate::tiling::Side::Right),
            "tile-maximize" => KeyAction::TileMaximize,
            "tile-restore" => KeyAction::TileRestore,
            "move-window-to-monitor-left" => KeyAction::MoveToOutput(crate::tiling::Side::Left),
            "move-window-to-monitor-right" => KeyAction::MoveToOutput(crate::tiling::Side::Right),
            "show-desktop" => KeyAction::ShowDesktop,
            "focus-top-bar" => KeyAction::FocusTopBar,
            "focus-dock" => KeyAction::FocusDock,
            "run-command" => KeyAction::RunPrompt,
            "move-window-to-next-workspace" => KeyAction::MoveWindowToNeighbour(true),
            "move-window-to-prev-workspace" => KeyAction::MoveWindowToNeighbour(false),
            "toggle-notification-center" => KeyAction::Shell(crate::topbar::TopbarAction::ToggleNotificationCenter),
            "toggle-quick-settings" => KeyAction::Shell(crate::topbar::TopbarAction::ToggleQuickSettings),
            "toggle-do-not-disturb" => KeyAction::Shell(crate::topbar::TopbarAction::ToggleDoNotDisturb),
            "toggle-caffeine" => KeyAction::Shell(crate::topbar::TopbarAction::ToggleCaffeine),
            other => {
                let Some(key) = crate::input::MediaKey::from_name(other) else {
                    tracing::warn!(action = cmd, "unknown simple key action");
                    return None;
                };
                KeyAction::Media(key)
            }
        }),
        KeyActionConfig::Complex(action) => {
            if let Some(idx) = action.switch_workspace {
                Some(KeyAction::SwitchWorkspace(idx.saturating_sub(1)))
            } else if let Some(idx) = action.move_window_to_workspace {
                Some(KeyAction::MoveWindowToWorkspace(idx.saturating_sub(1)))
            } else if let Some(cmd) = &action.exec {
                Some(crate::input::MediaKey::from_playerctl(cmd).map_or_else(
                    || KeyAction::Exec(cmd.clone()),
                    |key| {
                        tracing::info!(command = %cmd, ?key, "playerctl binding runs as a built-in media action");
                        KeyAction::Media(key)
                    },
                ))
            } else if let Some(vt) = action.vt {
                Some(KeyAction::VtSwitch(vt))
            } else {
                tracing::warn!(
                    "keybind action table is empty; expected one of \
                     switch-workspace, move-window-to-workspace, exec or vt"
                );
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dark_style_tile_rewrites_only_the_appearance() {
        use glide_theme::Appearance;
        let text = "# mine\n[topbar]\nappearance = \"dark\" # keep\nappearance-schedule = true\naccent = \"teal\"\n";
        let light = with_appearance(text, Appearance::Light).expect("valid toml");
        assert!(light.starts_with("# mine\n"));
        assert!(light.contains("appearance = \"light\" # keep"));
        assert!(light.contains("appearance-schedule = false"), "picking a style ends the schedule");
        assert!(light.contains("accent = \"teal\""));
        let parsed = Config::parse(&light).expect("still parses");
        assert_eq!(parsed.topbar.appearance, Appearance::Light);
        assert!(!parsed.topbar.appearance_schedule);

        let fresh = with_appearance("", Appearance::Dark).expect("empty file");
        assert!(fresh.contains("[topbar]") && fresh.contains("appearance = \"dark\""));
        assert!(!fresh.contains("appearance-schedule"));
        assert!(with_appearance("topbar = 3\n", Appearance::Dark).is_err());
        assert!(with_appearance("[topbar\n", Appearance::Dark).is_err());

        let dir = scratch_dir("appearance");
        let path = dir.join("glide.toml");
        write_appearance(&path, Appearance::Light).expect("written");
        let written = fs::read_to_string(&path).expect("readable");
        assert!(written.contains("appearance = \"light\""));
        assert_eq!(file_names(&dir), vec!["glide.toml".to_owned()], "no temporary file is left behind");
    }

    #[test]
    fn a_reload_rebuilds_the_keymap_only_when_the_keymap_changed() {
        let mut applied = AppliedConfig::default();
        let input = InputConfig::default();
        assert!(applied.keymap_changed(&input), "the first apply builds it");
        assert!(!applied.keymap_changed(&input), "an unrelated reload keeps the layout in use");
        let repeat = InputConfig { repeat_rate: 40, ..input.clone() };
        assert!(!applied.keymap_changed(&repeat));
        let layouts = InputConfig { xkb_layout: "us,de".to_string(), ..input.clone() };
        assert!(applied.keymap_changed(&layouts));
        let options = InputConfig { xkb_options: Some("caps:escape".to_string()), ..layouts };
        assert!(applied.keymap_changed(&options));
        assert!(applied.numlock_changed(false));
        assert!(!applied.numlock_changed(false));
        assert!(applied.numlock_changed(true));
    }

    #[test]
    fn displays_extend_unless_the_config_asks_for_a_mirror() {
        assert_eq!(Config::default().displays.arrangement, Arrangement::Extend);
        let mirror: Config = toml::from_str("[displays]\narrangement = \"mirror\"\n").expect("config");
        assert_eq!(mirror.displays.arrangement, Arrangement::Mirror);
        assert!(toml::from_str::<Config>("[displays]\narrangement = \"clone\"\n").is_err());
    }

    #[test]
    fn num_lock_is_on_unless_the_config_turns_it_off_so_the_keypad_types_digits() {
        assert!(InputConfig::default().numlock);
        let off: Config = toml::from_str("[input]\nnumlock = false\n").expect("config");
        assert!(!off.input.numlock);
    }

    #[test]
    fn gestures_claim_only_the_configured_finger_counts() {
        let three = GestureConfig::default();
        assert!(three.claims(3));
        assert!(three.claims(4), "the other count switches apps");
        assert!(!three.claims(2));
        let shell_only = GestureConfig { app_swipe: false, ..GestureConfig::default() };
        assert!(!shell_only.claims(4));
        let four = GestureConfig { fingers: 9, ..GestureConfig::default() };
        assert_eq!(four.fingers(), 4, "finger counts clamp to 3..=4");
        assert!(four.claims(4));
        let off = GestureConfig { enabled: false, ..GestureConfig::default() };
        assert!(!off.claims(3));
        let nothing = GestureConfig { workspace_swipe: false, overview_swipe: false, ..GestureConfig::default() };
        assert!(!nothing.claims(3), "with every shell gesture off, the swipe goes to the app");
    }

    #[test]
    fn gesture_deltas_follow_direction_and_clamped_sensitivity() {
        let natural = GestureConfig::default();
        assert_eq!(natural.delta(10.0, -4.0), (10.0, -4.0));
        let reversed = GestureConfig { natural: false, sensitivity: 9.0, ..GestureConfig::default() };
        assert_eq!(reversed.delta(10.0, -4.0), (-20.0, 8.0));
        let broken = GestureConfig { sensitivity: f64::NAN, ..GestureConfig::default() };
        assert_eq!(broken.delta(3.0, 3.0), (3.0, 3.0));
    }

    fn scratch_dir(tag: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("glide-config-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir is writable");
        dir
    }

    fn file_names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("dir is readable")
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn atomic_write_twice_keeps_the_latest_contents_and_leaves_no_temp_file() {
        let dir = scratch_dir("atomic");
        let target = dir.join("pins.toml");
        write_atomic(&target, b"first = 1\n").expect("first write succeeds");
        write_atomic(&target, b"second = 2\n").expect("second write succeeds");
        assert_eq!(fs::read_to_string(&target).expect("target exists"), "second = 2\n");
        assert_eq!(file_names(&dir), vec!["pins.toml".to_owned()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_config_falls_back_to_the_last_known_good_one() {
        let dir = scratch_dir("last-good");
        let path = dir.join("glide.toml");
        let good = "[workspaces]\nnew-window = \"active-workspace\"\n";
        fs::write(&path, good).expect("config is writable");

        let loaded = Config::load_from_path(&path);
        assert_eq!(loaded.workspaces.new_window, NewWindowPolicyConfig::ActiveWorkspace);
        assert_eq!(fs::read_to_string(last_good_path(&path)).expect("last-good was written"), good);

        fs::write(&path, "[workspaces\nthis is not toml").expect("config is writable");
        let recovered = Config::load_from_path(&path);
        assert_eq!(
            recovered.workspaces.new_window,
            NewWindowPolicyConfig::ActiveWorkspace,
            "a parse error must restore the last good config, not the defaults"
        );
        assert_eq!(fs::read_to_string(last_good_path(&path)).expect("last-good survives"), good);
        assert_eq!(file_names(&dir), vec!["glide.toml".to_owned(), "glide.toml.last-good".to_owned()]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_corrupt_config_without_a_last_good_copy_uses_defaults() {
        let dir = scratch_dir("no-last-good");
        let path = dir.join("glide.toml");
        fs::write(&path, "[workspaces\n").expect("config is writable");
        let loaded = Config::load_from_path(&path);
        assert_eq!(loaded.workspaces.new_window, NewWindowPolicyConfig::NewWorkspace);
        assert!(loaded.load_error.is_some(), "the failure is kept so the shell can tell the user");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_setting_from_a_newer_version_is_ignored_instead_of_discarding_the_file() {
        let dir = scratch_dir("unknown-keys");
        let path = dir.join("glide.toml");
        let text = "[general]\nblank-after-seconds = 120\nholograms = true\n\n[dock]\nautohide = \"always\"\n\n\
                    [telepathy]\nenabled = true\n";
        fs::write(&path, text).expect("config is writable");
        let loaded = Config::load_from_path(&path);
        assert_eq!(loaded.general.blank_after_seconds, 120);
        assert_eq!(loaded.dock.autohide, DockAutohide::Always);
        assert_eq!(loaded.load_error, None);
        let mut ignored = loaded.ignored_keys;
        ignored.sort();
        assert_eq!(ignored, ["general.holograms", "telepathy"]);
        assert!(Config::try_load_from_path(&path).is_ok(), "a reload accepts the same file");
        let wrong = "[general]\nblank-after-seconds = \"soon\"\n";
        fs::write(&path, wrong).expect("config is writable");
        assert!(Config::try_load_from_path(&path).is_err(), "a wrong value is still rejected");
        let _ = fs::remove_dir_all(&dir);
    }

    fn default_binding(mods: &[ModifierConfig], key: &str) -> Option<crate::input::KeyAction> {
        let keysym = parse_keysym(key)?;
        let has = |wanted: &[ModifierConfig]| mods.iter().any(|m| wanted.contains(m));
        Config::with_defaults()
            .resolve_keybinds()
            .into_iter()
            .find(|bind| {
                bind.keysym == keysym
                    && bind.modifiers.logo == has(&[ModifierConfig::Super, ModifierConfig::Logo])
                    && bind.modifiers.shift == has(&[ModifierConfig::Shift])
                    && bind.modifiers.ctrl == has(&[ModifierConfig::Ctrl, ModifierConfig::Control])
                    && bind.modifiers.alt == has(&[ModifierConfig::Alt])
            })
            .map(|bind| bind.action)
    }

    #[test]
    fn the_power_dialog_has_default_bindings() {
        assert_eq!(
            default_binding(&[ModifierConfig::Ctrl, ModifierConfig::Alt], "Delete"),
            Some(crate::input::KeyAction::PowerDialog)
        );
        assert_eq!(default_binding(&[], "XF86PowerOff"), Some(crate::input::KeyAction::PowerDialog));
        assert_eq!(
            default_binding(&[ModifierConfig::Super, ModifierConfig::Shift], "q"),
            Some(crate::input::KeyAction::Logout)
        );
        assert_eq!(default_binding(&[ModifierConfig::Super], "l"), Some(crate::input::KeyAction::Lock));
    }

    #[test]
    fn power_dialog_is_a_bindable_action() {
        let config: Config = toml::from_str(
            r#"
[[keybinds]]
mods = ["super"]
key = "Escape"
action = "power-dialog"
"#,
        )
        .expect("config parses");
        assert_eq!(
            config.resolve_keybinds().first().map(|bind| bind.action.clone()),
            Some(crate::input::KeyAction::PowerDialog)
        );
    }

    #[test]
    fn test_default_config() {
        let config = Config::with_defaults();
        assert_eq!(config.workspaces.new_window, NewWindowPolicyConfig::NewWorkspace);
        assert!(!config.keybinds.is_empty());
    }

    #[test]
    fn test_parse_valid_toml_keybinds() {
        let toml_str = r#"
[workspaces]
new-window = "active-workspace"

[[keybinds]]
mods = ["super"]
key = "Return"
action = { exec = "alacritty" }

[[keybinds]]
mods = ["super"]
key = "q"
action = "quit"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.workspaces.new_window, NewWindowPolicyConfig::ActiveWorkspace);
        assert_eq!(config.keybinds.len(), 2);
        assert_eq!(
            config.keybinds[0].action,
            KeyActionConfig::Complex(ParameterizedAction {
                exec: Some("alacritty".to_string()),
                ..ParameterizedAction::default()
            })
        );
    }

    #[test]
    fn animations_default_to_spec_timing_and_disabling_wins_over_speed() {
        let scale = |text: &str| toml::from_str::<Config>(text).unwrap().animations.motion_scale();
        assert!(Config::with_defaults().animations.enabled);
        assert!((scale("") - 1.0).abs() < f32::EPSILON);
        assert!((scale("[animations]\nenabled = true\nspeed = 0.5\n") - 0.5).abs() < f32::EPSILON);
        assert!(scale("[animations]\nenabled = false\nspeed = 3.0\n").abs() < f32::EPSILON);
    }

    #[test]
    fn misspelled_keys_and_invalid_values_are_rejected_not_swallowed() {
        for (text, culprit) in [
            ("[animations]\nspeeed = 2.0\n", "speeed"),
            ("[input]\ntap-to-clik = true\n", "tap-to-clik"),
            ("[workspacs]\nnew-window = \"active-workspace\"\n", "workspacs"),
            ("[general]\nbackgroud = \"#101020\"\n", ""),
            ("[input]\nscroll-method = \"wheel\"\n", ""),
            ("[general]\nwallpaper-fit = \"stretch\"\n", ""),
            ("[decorations]\nbackground = \"not-a-color\"\n", ""),
            ("[[keybinds]]\nmods = [\"super\"]\nkey = \"Return\"\naction = { exce = \"x\" }\n", ""),
        ] {
            let err = toml::from_str::<Config>(text).expect_err(text);
            assert!(err.to_string().contains(culprit), "{err}");
        }
    }

    mod pointer_devices {
        use super::*;

        #[test]
        fn defaults_keep_the_behaviour_from_before_the_per_device_keys() {
            let input = InputConfig::default();
            assert!(!input.left_handed);
            assert!(!input.middle_emulation);
            assert!(input.disable_while_typing);
            assert!(input.smooth_wheel && input.smooth_wheel_exclude.is_empty());
            assert!(input.tap_and_drag);
            assert_eq!(input.scroll_method, ScrollMethodConfig::TwoFinger);
            assert_eq!(input.click_method, None);
            for touchpad in [false, true] {
                let pointer = input.pointer_settings(touchpad);
                assert!((pointer.accel_speed - 0.0).abs() < f64::EPSILON);
                assert_eq!(pointer.accel_profile, AccelProfileConfig::Adaptive);
                assert!(!pointer.natural_scroll);
                assert!((pointer.scroll_factor - 1.0).abs() < f64::EPSILON);
            }
        }

        #[test]
        fn every_new_input_key_parses() {
            let config: Config = toml::from_str(
                "[input]\nleft-handed = true\nmiddle-emulation = true\ndisable-while-typing = false\ntap-and-drag = false\nscroll-method = \"none\"\nclick-method = \"areas\"\nscroll-factor = 2.0\ntouchpad-accel-speed = 0.5\ntouchpad-accel-profile = \"flat\"\ntouchpad-natural-scroll = true\ntouchpad-scroll-factor = 0.5\nsmooth-wheel = false\nsmooth-wheel-exclude = [\"org.kde.dolphin\"]\n",
            )
            .unwrap();
            let input = &config.input;
            assert!(!input.smooth_wheel);
            assert_eq!(input.smooth_wheel_exclude, ["org.kde.dolphin"]);
            assert!(input.left_handed && input.middle_emulation);
            assert!(!input.disable_while_typing && !input.tap_and_drag);
            assert_eq!(input.scroll_method, ScrollMethodConfig::None);
            assert_eq!(input.click_method, Some(ClickMethodConfig::Areas));
            assert!((input.scroll_factor - 2.0).abs() < f64::EPSILON);
            assert_eq!(input.touchpad_accel_speed, Some(0.5));
            assert_eq!(input.touchpad_accel_profile, Some(AccelProfileConfig::Flat));
            assert_eq!(input.touchpad_natural_scroll, Some(true));
            assert_eq!(input.touchpad_scroll_factor, Some(0.5));
        }

        #[test]
        fn touchpad_overrides_fall_back_to_the_shared_pointer_keys() {
            let shared_only =
                InputConfig { accel_speed: 0.4, natural_scroll: true, scroll_factor: 2.0, ..InputConfig::default() };
            assert_eq!(
                shared_only.pointer_settings(true),
                shared_only.pointer_settings(false),
                "old configs keep meaning every pointer"
            );

            let split = InputConfig {
                touchpad_accel_speed: Some(-0.5),
                touchpad_accel_profile: Some(AccelProfileConfig::Flat),
                touchpad_natural_scroll: Some(false),
                touchpad_scroll_factor: Some(0.5),
                ..shared_only
            };
            let mouse = split.pointer_settings(false);
            let touchpad = split.pointer_settings(true);
            assert!((mouse.accel_speed - 0.4).abs() < f64::EPSILON);
            assert!(mouse.natural_scroll);
            assert!((touchpad.accel_speed + 0.5).abs() < f64::EPSILON);
            assert_eq!(touchpad.accel_profile, AccelProfileConfig::Flat);
            assert!(!touchpad.natural_scroll);
            assert!((touchpad.scroll_factor - 0.5).abs() < f64::EPSILON);
        }

        #[test]
        fn out_of_range_values_are_clamped_for_the_device() {
            let wild = InputConfig {
                accel_speed: 7.0,
                scroll_factor: 100.0,
                touchpad_scroll_factor: Some(f64::NAN),
                ..InputConfig::default()
            };
            assert!((wild.pointer_settings(false).accel_speed - 1.0).abs() < f64::EPSILON);
            assert!((wild.pointer_settings(false).scroll_factor - 5.0).abs() < f64::EPSILON);
            assert!((wild.pointer_settings(true).scroll_factor - 1.0).abs() < f64::EPSILON);
        }
    }

    #[test]
    fn input_section_parses_and_defaults_are_sane() {
        let toml_str = r#"
[input]
xkb-layout = "us,ua"
xkb-options = "grp:alt_shift_toggle"
repeat-rate = 40
natural-scroll = true
accel-profile = "flat"
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.input.xkb_layout, "us,ua");
        assert_eq!(config.input.xkb_options.as_deref(), Some("grp:alt_shift_toggle"));
        assert_eq!(config.input.repeat_rate, 40);
        assert_eq!(config.input.repeat_delay, 200);
        assert!(config.input.natural_scroll);
        assert!(config.input.tap_to_click);
        assert_eq!(config.input.accel_profile, AccelProfileConfig::Flat);

        let defaults = InputConfig::default();
        assert!(defaults.tap_to_click);
        assert!(!defaults.natural_scroll);
        assert_eq!(defaults.xkb_layout, "us,ua");
        assert_eq!(defaults.xkb_options.as_deref(), Some("grp:win_space_toggle"));
    }

    #[test]
    fn output_sections_parse() {
        let toml_str = r#"
[[output]]
name = "eDP-1"
scale = 2.0

[[output]]
name = "DP-3"
position = [2560, 0]
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.outputs.len(), 2);
        assert_eq!(config.outputs[0].name, "eDP-1");
        assert_eq!(config.outputs[0].scale, Some(2.0));
        assert_eq!(config.outputs[0].position, None);
        assert_eq!(config.outputs[1].position, Some([2560, 0]));
    }

    #[test]
    fn mode_specs_parse_size_and_optional_refresh() {
        assert_eq!(
            ModeSpec::parse("2560x1440@143.91"),
            Some(ModeSpec { width: 2560, height: 1440, refresh_mhz: Some(143_910) })
        );
        assert_eq!(ModeSpec::parse(" 1920X1080 "), Some(ModeSpec { width: 1920, height: 1080, refresh_mhz: None }));
        assert_eq!(ModeSpec::parse("1280x720@60Hz").and_then(|spec| spec.refresh_mhz), Some(60_000));
        for broken in ["", "1920", "0x1080", "axb", "1920x1080@", "1920x1080@-5"] {
            assert_eq!(ModeSpec::parse(broken), None, "{broken}");
        }
    }

    #[test]
    fn mode_specs_pick_the_closest_matching_mode() {
        let modes = [
            (2560, 1440, 59_951, true),
            (2560, 1440, 143_912, false),
            (1920, 1080, 60_000, false),
            (1920, 1080, 50_000, false),
        ];
        let pick = |text: &str| ModeSpec::parse(text).and_then(|spec| spec.pick(&modes));
        assert_eq!(pick("2560x1440@144"), Some(1));
        assert_eq!(pick("2560x1440@59.95"), Some(0));
        assert_eq!(pick("2560x1440"), Some(0), "the preferred mode wins");
        assert_eq!(pick("1920x1080"), Some(2), "else the fastest");
        assert_eq!(pick("1920x1080@75"), None, "never a different refresh");
        assert_eq!(pick("3840x2160"), None);
    }

    #[test]
    fn the_general_section_defaults_and_parses() {
        let general = GeneralConfig::default();
        assert_eq!(general.background, Color::from_hex("#222c3cff").unwrap());
        assert!(general.autostart.is_empty() && general.xdg_autostart && general.polkit_agent);
        assert!(general.pause_media_on_disconnect, "headphones going away pause the music unless turned off");
        assert_eq!((general.wallpaper, general.wallpaper_fit), (None, WallpaperFit::Cover));

        let config: Config = toml::from_str(
            "[general]\nbackground = \"#101020\"\npolkit-agent = false\npause-media-on-disconnect = false\nautostart = [\"foot\", \"waybar --config /tmp/bar.json\"]\nwallpaper = \"/home/user/Pictures/wall.png\"\nwallpaper-fit = \"contain\"\n",
        )
        .unwrap();
        let general = config.general;
        assert_eq!(general.background, Color::from_hex("#101020").unwrap());
        assert!(!general.polkit_agent && !general.pause_media_on_disconnect);
        assert_eq!(general.autostart, vec!["foot", "waybar --config /tmp/bar.json"]);
        assert_eq!(general.wallpaper, Some(PathBuf::from("/home/user/Pictures/wall.png")));
        assert_eq!(general.wallpaper_fit, WallpaperFit::Contain);
        let emptied: Config = toml::from_str("[general]\nautostart = []\n").unwrap();
        assert!(emptied.general.autostart.is_empty());
    }

    #[test]
    fn decoration_theme_parses_from_toml() {
        let config: Config = toml::from_str(
            "[decorations]\nheight = 40\ntitle-align = \"center\"\nclose-icon = \"#cccccc\"\nborder-width = 2\nshadow = false\nshadow-color = \"#000000aa\"\n",
        )
        .unwrap();
        let d = &config.decorations;
        assert_eq!((d.height, d.title_align, d.border_width), (40, glide_theme::TitleAlign::Center, 2));
        assert_eq!(d.close_icon, Color::from_hex("#cccccc").unwrap());
        assert!(!d.shadow);
        assert_eq!(d.shadow_color, Color::from_hex("#000000aa").unwrap());
    }

    #[test]
    fn the_night_light_schedule_wraps_past_midnight() {
        let schedule = NightLightConfig {
            schedule: true,
            from: "20:00".to_string(),
            to: "07:00".to_string(),
            ..NightLightConfig::default()
        };
        assert_eq!(schedule.wants_on(20 * 60), Some(true));
        assert_eq!(schedule.wants_on(23 * 60 + 59), Some(true));
        assert_eq!(schedule.wants_on(6 * 60 + 59), Some(true));
        assert_eq!(schedule.wants_on(7 * 60), Some(false));
        assert_eq!(schedule.wants_on(12 * 60), Some(false));

        let daytime = NightLightConfig { from: "09:00".to_string(), to: "17:00".to_string(), ..schedule };
        assert_eq!(daytime.wants_on(12 * 60), Some(true));
        assert_eq!(daytime.wants_on(18 * 60), Some(false));

        let off = NightLightConfig::default();
        assert_eq!(off.wants_on(22 * 60), None, "no schedule means no opinion");
    }

    #[test]
    fn a_malformed_clock_disables_the_schedule_instead_of_panicking() {
        let broken = NightLightConfig {
            schedule: true,
            from: "25:99".to_string(),
            to: "07".to_string(),
            ..NightLightConfig::default()
        };
        assert_eq!(broken.window(), None);
        assert_eq!(broken.wants_on(0), None);
    }

    #[test]
    fn night_light_tint_is_neutral_at_daylight_and_warms_monotonically() {
        assert_eq!(night_light_tint(NIGHT_LIGHT_NEUTRAL_KELVIN), (1.0, 1.0, 1.0));
        assert_eq!(night_light_tint(9000), (1.0, 1.0, 1.0));
        let mut previous = night_light_tint(NIGHT_LIGHT_NEUTRAL_KELVIN);
        for kelvin in (NIGHT_LIGHT_MIN_KELVIN..NIGHT_LIGHT_NEUTRAL_KELVIN).rev().step_by(100) {
            let tint = night_light_tint(kelvin);
            assert!(tint.1 <= previous.1 && tint.2 <= previous.2, "{kelvin} K");
            assert!(tint.2 <= tint.1, "blue is cut harder than green at {kelvin} K");
            previous = tint;
        }
        let (_, green, blue) = night_light_tint(NIGHT_LIGHT_DEFAULT_KELVIN);
        assert!((0.75..0.9).contains(&green) && (0.55..0.75).contains(&blue));
    }

    #[test]
    fn night_light_ramp_scales_each_channel_and_ends_at_the_tint() {
        let ramp = night_light_ramp(256, (1.0, 0.5, 0.25));
        assert_eq!(ramp.len(), 768);
        assert_eq!((ramp[0], ramp[256], ramp[512]), (0, 0, 0));
        assert_eq!(ramp[255], u16::MAX);
        assert_eq!(ramp[511], 32768);
        assert_eq!(ramp[767], 16384);
        assert!(night_light_ramp(1, (1.0, 1.0, 1.0)).iter().all(|v| *v == 0));
    }

    #[test]
    fn topbar_animation_night_light_and_overview_extras_parse_from_toml() {
        assert_eq!(NightLightConfig::default().temperature, NIGHT_LIGHT_DEFAULT_KELVIN);
        let config: Config = toml::from_str(
            "[topbar]\nbattery-low-pct = 15\nmax-volume-pct = 130\n\n[animations]\nrespect-system = false\n\n[general]\nlanguage = \"en\"\n\n[night-light]\nschedule = true\nfrom = \"21:30\"\nto = \"06:00\"\ntemperature = 3200\n\n[overview]\ngap = 40\nlabel = false\nclose-hover-foreground = \"#0e0e0e\"\nduration-ms = 150\n",
        )
        .unwrap();
        assert_eq!(config.topbar.battery_low_pct, 15);
        assert_eq!(config.topbar.max_volume_pct, 130);
        assert!(!config.animations.respect_system);
        assert_eq!(config.general.language, Some(crate::i18n::Lang::En));
        assert_eq!(config.night_light.window(), Some((21 * 60 + 30, 6 * 60)));
        assert_eq!(config.night_light.temperature, 3200);
        assert_eq!(config.topbar.theme().battery_low_pct, 15);
        let o = &config.overview;
        assert_eq!((o.gap, o.label, o.duration_ms), (40, false, 150));
        assert_eq!(o.close_hover_foreground, Color::from_hex("#0e0e0e").unwrap());
    }

    #[test]
    fn the_retired_topbar_switch_still_parses_so_old_files_are_not_repaired_noisily() {
        let config: Config = toml::from_str("[topbar]\nenabled = false\ntray = false\n").unwrap();
        assert!(!config.topbar.tray, "deny_unknown_fields would have refused the whole table");
    }

    #[test]
    fn invalid_config_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("glide-config-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join("glide.toml");
        std::fs::write(&path, "this is not valid toml {{{").expect("write config");

        let config = Config::load_from_path(&path);

        assert!(config.load_error.as_deref().is_some_and(|error| error.starts_with("failed to parse")));
        assert!(Config { load_error: None, ..config } == Config::with_defaults(), "everything else is the default");
        let _ = std::fs::remove_dir_all(&dir);
    }

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

    #[test]
    #[allow(clippy::too_many_lines)] // One fixture listing every key Settings writes, kept in one place
    fn every_key_written_by_glide_settings_is_accepted() {
        let text = r##"
[general]
language = "en"
wallpaper = "/tmp/wall.jpg"
wallpaper-fit = "contain"
background = "#1d2433"
font-family = "Inter"
blank-after-seconds = 300
lock-after-seconds = 60
suspend-after-seconds = 3600
suspend-on-battery-after-seconds = 900
autostart = ["nm-applet", "blueman-applet"]
xdg-autostart = true
polkit-agent = false

[topbar]
enabled = true
appearance = "light"
accent = "orange"
battery-low-pct = 15
max-volume-pct = 150
global-menu = false
appmenu-registrar = false
battery-percentage = false
clock-24h = false
clock-date = false

[dock]
enabled = true
autohide = "always"
icon-theme = "Adwaita"
item-size = 64
icon-size = 50
hover-scale = 1.2
hover-lift = 6
bottom-margin = 16

[decorations]
height = 38
title-align = "center"
shadow = false
border-width = 0
double-click = "minimize"
button-layout = "close-maximize"

[overview]
label = false
hot-corner = true

[workspaces]
new-window = "active-workspace"

[windows]
edge-snapping = false

[animations]
enabled = true
speed = 1.25
respect-system = false

[night-light]
schedule = true
from = "21:30"
to = "06:45"
temperature = 3400

[input]
xkb-layout = "us,ua,pl"
xkb-variant = ",,"
xkb-options = "grp:alt_shift_toggle,caps:escape"
layout-scope = "application"
new-window-layout = "current"
repeat-delay = 300
repeat-rate = 40
numlock = true
tap-to-click = false
natural-scroll = true
accel-profile = "flat"
accel-speed = 0.35
left-handed = true
middle-emulation = true
disable-while-typing = false
tap-and-drag = false
scroll-method = "edge"
click-method = "clickfinger"
scroll-factor = 1.5
touchpad-accel-speed = -0.2
touchpad-accel-profile = "adaptive"
touchpad-natural-scroll = true
touchpad-scroll-factor = 0.75

[notifications]
do-not-disturb = true
schedule = true
from = "22:30"
to = "07:15"
allow-critical = false
banner-seconds = 12
position = "top-center"
muted-apps = ["org.telegram.desktop", "Steam"]
sounds = false

[gestures]
enabled = true
fingers = 4
workspace-swipe = false
overview-swipe = true
app-swipe = false
pinch = false
touch-edges = false
natural = false
sensitivity = 1.5

[accessibility]
cursor-size = 48

[[output]]
name = "eDP-1"
scale = 1.25
mode = "2256x1504@59.999"

[[output]]
name = "HDMI-A-1"
make = "Dell"
model = "U2720Q"
position = [2256, 0]
mode = "1920x1080"

[[keybinds]]
mods = ["super", "shift"]
key = "Return"
action = "toggle-launcher"

[[keybinds]]
mods = ["ctrl", "alt"]
key = "t"
action = { exec = "alacritty" }

[[keybinds]]
mods = ["super"]
key = "1"
action = { switch-workspace = 1 }

[[keybinds]]
mods = ["super", "shift"]
key = "1"
action = { move-window-to-workspace = 1 }

[[keybinds]]
mods = ["ctrl", "alt"]
key = "F2"
action = { vt = 2 }

[[keybinds]]
mods = []
key = "XF86AudioMute"
action = { exec = "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle" }
"##;
        let config: Config = toml::from_str(text).expect("settings output must parse");
        assert_eq!(config.gestures.fingers(), 4);
        assert!(!config.gestures.workspace_swipe);
        assert!(!config.gestures.app_swipe);
        assert!(!config.gestures.pinch);
        assert!(!config.gestures.touch_edges);
        assert!(!config.gestures.natural);
        assert!((config.gestures.sensitivity - 1.5).abs() < f64::EPSILON);
        assert_eq!(config.topbar.appearance, glide_theme::Appearance::Light);
        assert_eq!(config.topbar.accent, glide_theme::Accent::Orange);
        assert_eq!(config.topbar.max_volume_pct, 150);
        assert!(!config.topbar.global_menu);
        assert_eq!(config.dock.theme.item_size, 64);
        assert_eq!(config.dock.autohide, DockAutohide::Always);
        assert_eq!(config.dock.theme.icon_theme.as_deref(), Some("Adwaita"));
        assert_eq!(config.decorations.height, 38);
        assert_eq!(config.decorations.title_align, glide_theme::TitleAlign::Center);
        assert!(!config.decorations.shadow);
        assert_eq!(config.decorations.double_click, glide_theme::TitlebarAction::Minimize);
        assert_eq!(config.decorations.button_layout, glide_theme::ButtonLayout::CloseMaximize);
        assert!(!config.overview.label);
        assert!(config.overview.hot_corner);
        assert!(!config.windows.edge_snapping);
        assert!((config.dock.theme.hover_scale - 1.2).abs() < f64::EPSILON);
        assert_eq!(config.dock.theme.hover_lift, 6);
        assert_eq!(config.dock.theme.bottom_margin, 16);
        assert_eq!(config.general.background, Color::from_hex("#1d2433").unwrap());
        assert_eq!(config.general.blank_after_seconds, 300);
        assert_eq!(config.general.autostart.len(), 2);
        assert!(config.general.xdg_autostart);
        assert!(!config.general.polkit_agent);
        assert_eq!(config.topbar.clock_24h, Some(false));
        assert!(!config.topbar.clock_date);
        assert_eq!(config.general.font_family.as_deref(), Some("Inter"));
        assert_eq!(config.input.accel_profile, AccelProfileConfig::Flat);
        assert!(config.input.numlock);
        assert!(config.input.left_handed);
        assert_eq!(config.input.scroll_method, ScrollMethodConfig::Edge);
        assert_eq!(config.input.click_method, Some(ClickMethodConfig::Clickfinger));
        assert_eq!(config.input.touchpad_scroll_factor, Some(0.75));
        let policy = config.notifications.policy();
        assert!(policy.do_not_disturb);
        assert_eq!(policy.schedule, Some((22 * 60 + 30, 7 * 60 + 15)));
        assert!(!policy.allow_critical);
        assert_eq!(policy.banner_timeout, Some(std::time::Duration::from_secs(12)));
        assert_eq!(policy.position, crate::notifications::BannerPosition::Center);
        assert_eq!(policy.muted.len(), 2);
        assert_eq!(config.accessibility.cursor_size, Some(48));
        assert_eq!(config.outputs[0].scale, Some(1.25));
        assert_eq!(
            config.outputs[0].mode_spec(),
            Some(ModeSpec { width: 2256, height: 1504, refresh_mhz: Some(59_999) })
        );
        assert_eq!(config.outputs[1].position, Some([2256, 0]));
        assert_eq!(config.night_light.temperature, 3400);
        assert!(!config.topbar.battery_percentage);
        assert_eq!(config.keybinds.len(), 6);
        assert_eq!(config.resolve_keybinds().len(), 6, "every keybind form Settings writes must resolve to an action");
    }

    #[test]
    fn keybinds_saved_with_playerctl_run_the_built_in_media_actions() {
        use crate::input::{KeyAction, MediaKey};
        let text = "[[keybinds]]\nmods = []\nkey = \"XF86AudioPlay\"\naction = { exec = \"playerctl play-pause\" }\n\n\
                    [[keybinds]]\nmods = []\nkey = \"XF86AudioNext\"\naction = { exec = \"playerctl next\" }\n\n\
                    [[keybinds]]\nmods = []\nkey = \"XF86AudioPause\"\naction = \"media-pause\"\n\n\
                    [[keybinds]]\nmods = []\nkey = \"F9\"\naction = { exec = \"playerctl --player=mpv next\" }\n";
        let config: Config = toml::from_str(text).expect("parses");
        let actions: Vec<KeyAction> = config.resolve_keybinds().into_iter().map(|bind| bind.action).collect();
        assert_eq!(
            actions,
            [
                KeyAction::Media(MediaKey::PlayPause),
                KeyAction::Media(MediaKey::Next),
                KeyAction::Media(MediaKey::Pause),
                KeyAction::Exec("playerctl --player=mpv next".into()),
            ]
        );
    }

    #[test]
    fn glide_settings_mirrors_the_default_keybinds_exactly() {
        let text = include_str!("../../glide-settings/src/services/default_keybinds.toml");
        let config: Config = toml::from_str(text).expect("the settings mirror parses");
        assert_eq!(
            config.keybinds,
            Config::default_keybinds(),
            "Settings writes the whole [[keybinds]] list, so its copy of the defaults must match"
        );
    }

    #[test]
    fn reload_leaves_client_decorated_windows_without_a_titlebar() {
        let mut layout = glide_layout::Layout::new();
        let csd = glide_layout::WindowId::next();
        let ssd = glide_layout::WindowId::next();
        layout.add_window(csd, glide_layout::Size::new(800, 600));
        layout.add_window(ssd, glide_layout::Size::new(800, 600));
        layout.set_window_insets(ssd, glide_layout::Insets { top: 48, ..glide_layout::Insets::NONE });

        let touched = server_decorated(&layout, [csd, ssd].into_iter());

        assert_eq!(touched, vec![ssd], "a reload must never give a CSD window (Chrome, GTK) a second titlebar");
    }

    #[test]
    fn desktop_appearance_maps_to_gnome_values() {
        use crate::system_info::{gnome_accent, gnome_color_scheme, gtk_theme_for};
        assert_eq!(gnome_color_scheme(glide_theme::Appearance::Dark), "prefer-dark");
        assert_eq!(gnome_accent(glide_theme::Accent::Amber), "yellow");
        assert_eq!(gnome_accent(glide_theme::Accent::Graphite), "slate");
        assert_eq!(gtk_theme_for("'Adwaita'", glide_theme::Appearance::Dark), Some("Adwaita-dark"));
        assert_eq!(gtk_theme_for("'Adwaita-dark'\n", glide_theme::Appearance::Light), Some("Adwaita"));
        assert_eq!(gtk_theme_for("'Arc-Dark'", glide_theme::Appearance::Light), None);
    }

    #[test]
    fn an_output_section_follows_its_monitor_across_ports_and_skips_a_stranger() {
        let config: Config = toml::from_str(
            "[[output]]\nname = \"DP-1\"\nmake = \"Dell\"\nmodel = \"U2720Q\"\nscale = 1.5\n\n\
             [[output]]\nname = \"DP-2\"\nscale = 1.25\n\n\
             [[output]]\nname = \"HDMI-A-1\"\nmake = \"Dell\"\nmodel = \"U2720Q\"\nscale = 2.0\n",
        )
        .expect("valid outputs");
        let identity = |name: &str, make: &str, model: &str| OutputIdentity {
            name: name.into(),
            make: make.into(),
            model: model.into(),
            serial: "Unknown".into(),
        };
        let scale = |id: &OutputIdentity| output_config(&config.outputs, id).and_then(|config| config.scale);
        assert_eq!(scale(&identity("DP-1", "Dell", "U2720Q")), Some(1.5), "its own port wins a tie");
        assert_eq!(scale(&identity("HDMI-A-1", "Dell", "U2720Q")), Some(2.0));
        assert_eq!(scale(&identity("DP-3", "Dell", "U2720Q")), Some(1.5), "a known monitor on a new port");
        assert_eq!(scale(&identity("DP-2", "LG", "27UL850")), Some(1.25), "a name-only section still applies");
        assert_eq!(scale(&identity("DP-1", "LG", "27UL850")), None, "another monitor on DP-1 does not inherit it");
    }

    #[test]
    fn the_idle_policy_locks_with_the_blank_by_default_and_reads_its_overrides() {
        use std::time::Duration;
        assert_eq!(Config::with_defaults().general.idle_policy(), crate::idle::Policy::default());
        let config: Config = toml::from_str(
            "[general]\nblank-after-seconds = 0\nlock-after-seconds = -1\nsuspend-after-seconds = 1800\n\
             suspend-on-battery-after-seconds = 600\n",
        )
        .expect("valid idle keys");
        let policy = config.general.idle_policy();
        assert_eq!((policy.blank, policy.lock), (None, None), "0 never blanks, a negative delay never locks");
        assert_eq!(policy.suspend, Some(Duration::from_mins(30)));
        assert_eq!(policy.suspend_on_battery, Some(Duration::from_mins(10)));
        let delayed: Config = toml::from_str("[general]\nlock-after-seconds = 30\n").expect("valid idle keys");
        assert_eq!(delayed.general.idle_policy().lock, Some(Duration::from_secs(30)));
    }

    #[test]
    fn a_scheduled_appearance_is_dark_from_seven_in_the_evening_to_seven_in_the_morning() {
        use glide_theme::Appearance::{Dark, Light};
        let at = |hours: u16, minutes: u16| scheduled_appearance(hours * 60 + minutes);
        assert_eq!(at(6, 59), Dark);
        assert_eq!(at(7, 0), Light);
        assert_eq!(at(18, 59), Light);
        assert_eq!(at(19, 0), Dark);
        assert_eq!(at(0, 0), Dark);
        let fixed = TopbarConfig { appearance: Light, ..TopbarConfig::default() };
        assert_eq!(fixed.effective_appearance(Some(23 * 60)), Light, "without the schedule the setting stands");
        let scheduled = TopbarConfig { appearance_schedule: true, ..fixed };
        assert_eq!(scheduled.effective_appearance(Some(23 * 60)), Dark);
        assert_eq!(scheduled.effective_appearance(None), Light, "an unreadable clock keeps the setting");
    }
}
