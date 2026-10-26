use crate::{
    i18n::{Key, t},
    icon::DesktopApp,
};

use super::history::LaunchHistory;

pub const FREQUENT_LIMIT: usize = 8;
pub const LIST_APP_LIMIT: usize = 10;
pub const LIST_ACTION_LIMIT: usize = 5;
pub const LIST_COMMAND_LIMIT: usize = 5;
pub const LIST_SETTINGS_LIMIT: usize = 5;
pub const LIST_WINDOW_LIMIT: usize = 5;
pub const LIST_FILE_LIMIT: usize = 5;

const SCORE_NAME_PREFIX: u32 = 1000;
const SCORE_NAME_WORD: u32 = 800;
const SCORE_NAME_SUBSTRING: u32 = 600;
const SCORE_KEYWORD_PREFIX: u32 = 400;
const SCORE_KEYWORD_SUBSTRING: u32 = 300;
const SCORE_COMMENT: u32 = 150;
const FREQUENCY_BONUS_MAX: u32 = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MatchRank {
    Prefix,
    WordPrefix,
    Substring,
}

const fn name_score(rank: MatchRank) -> u32 {
    match rank {
        MatchRank::Prefix => SCORE_NAME_PREFIX,
        MatchRank::WordPrefix => SCORE_NAME_WORD,
        MatchRank::Substring => SCORE_NAME_SUBSTRING,
    }
}

const fn keyword_score(rank: MatchRank) -> u32 {
    match rank {
        MatchRank::Prefix | MatchRank::WordPrefix => SCORE_KEYWORD_PREFIX,
        MatchRank::Substring => SCORE_KEYWORD_SUBSTRING,
    }
}

#[must_use]
pub fn match_rank(haystack: &str, needle: &str) -> Option<MatchRank> {
    if needle.is_empty() {
        return None;
    }
    let haystack = haystack.to_lowercase();
    if haystack.starts_with(needle) {
        return Some(MatchRank::Prefix);
    }
    if haystack
        .split(|c: char| c.is_whitespace() || matches!(c, '-' | '_' | '.' | '/' | '(' | ')'))
        .any(|word| word.starts_with(needle))
    {
        return Some(MatchRank::WordPrefix);
    }
    haystack.contains(needle).then_some(MatchRank::Substring)
}

fn binary_name(command: &str) -> &str {
    command
        .split_whitespace()
        .find(|part| !part.contains('='))
        .and_then(|part| part.rsplit('/').next())
        .unwrap_or_default()
}

#[must_use]
pub fn text_score(app: &DesktopApp, needle: &str) -> Option<u32> {
    if let Some(rank) = match_rank(&app.name, needle) {
        return Some(name_score(rank));
    }
    let secondary = std::iter::once(app.generic_name.as_str())
        .chain(app.keywords.iter().map(String::as_str))
        .chain(std::iter::once(binary_name(&app.command)))
        .chain(std::iter::once(app.id.as_str()))
        .filter_map(|text| match_rank(text, needle))
        .min();
    if let Some(rank) = secondary {
        return Some(keyword_score(rank));
    }
    match_rank(&app.comment, needle)
        .filter(|rank| *rank != MatchRank::Substring || needle.chars().count() >= 3)
        .map(|_| SCORE_COMMENT)
}

#[must_use]
pub fn frequency_bonus(frecency: f64) -> u32 {
    if frecency <= 0.0 {
        return 0;
    }
    let bonus = (40.0 * (1.0 + frecency).ln()).round() as u32;
    bonus.min(FREQUENCY_BONUS_MAX)
}

#[must_use]
pub fn rank_apps(
    apps: &[DesktopApp], candidates: &[usize], query: &str, history: &LaunchHistory, now: u64,
) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return candidates.to_vec();
    }
    let mut scored: Vec<(u32, usize)> = candidates
        .iter()
        .filter_map(|&index| {
            let app = apps.get(index)?;
            let score = text_score(app, &needle)?;
            Some((score + frequency_bonus(history.frecency(&app.id, now)), index))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, index)| index).collect()
}

#[must_use]
pub fn frequent_first(
    apps: &[DesktopApp], candidates: &[usize], history: &LaunchHistory, now: u64, limit: usize,
) -> Vec<usize> {
    let mut frequent: Vec<(f64, usize)> = candidates
        .iter()
        .filter_map(|&index| {
            let score = history.frecency(&apps.get(index)?.id, now);
            (score > 0.0).then_some((score, index))
        })
        .collect();
    frequent.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
    frequent.truncate(limit);
    let mut ordered: Vec<usize> = frequent.iter().map(|&(_, index)| index).collect();
    ordered.extend(candidates.iter().copied().filter(|index| !frequent.iter().any(|&(_, chosen)| chosen == *index)));
    ordered
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SystemCommand {
    Lock,
    LogOut,
    Restart,
    ShutDown,
    Sleep,
    ScreenMirroring,
}

impl SystemCommand {
    pub const ALL: [Self; 6] =
        [Self::Lock, Self::LogOut, Self::Restart, Self::ShutDown, Self::Sleep, Self::ScreenMirroring];

    #[must_use]
    pub fn title(self) -> &'static str {
        t(match self {
            Self::Lock => Key::LockScreen,
            Self::LogOut => Key::CommandLogOut,
            Self::Restart => Key::CommandRestart,
            Self::ShutDown => Key::CommandShutDown,
            Self::Sleep => Key::CommandSleep,
            Self::ScreenMirroring => Key::CastTitle,
        })
    }

    #[must_use]
    pub const fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Lock => &["lock", "lock screen", "заблокувати", "блокування"],
            Self::LogOut => &["log out", "logout", "sign out", "exit", "quit", "вийти", "вихід"],
            Self::Restart => &["restart", "reboot", "перезавантажити", "перезапуск"],
            Self::ShutDown => &["shut down", "shutdown", "power off", "poweroff", "вимкнути", "вимкнення"],
            Self::Sleep => &["sleep", "suspend", "сон", "призупинити", "режим сну"],
            Self::ScreenMirroring => &[
                "screen mirroring",
                "mirror",
                "cast",
                "miracast",
                "tv",
                "дублювання екрана",
                "трансляція",
                "телевізор",
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettingsPanel {
    Network,
    Bluetooth,
    Vpn,
    Netbird,
    Notifications,
    Sound,
    Appearance,
    Wallpaper,
    Desktop,
    TopBar,
    Windows,
    Multitasking,
    Displays,
    NightLight,
    Power,
    ScreenShare,
    Miracast,
    Printers,
    Keyboard,
    Shortcuts,
    Mouse,
    Gestures,
    Accessibility,
    LockScreen,
    Privacy,
    Users,
    DateTime,
    Region,
    Apps,
    Startup,
    Storage,
    About,
}

fn netbird_installed() -> bool {
    static INSTALLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *INSTALLED.get_or_init(glide_netbird::installed)
}

impl SettingsPanel {
    pub const ALL: [Self; 32] = [
        Self::Network,
        Self::Bluetooth,
        Self::Vpn,
        Self::Netbird,
        Self::Notifications,
        Self::Sound,
        Self::Appearance,
        Self::Wallpaper,
        Self::Desktop,
        Self::TopBar,
        Self::Windows,
        Self::Multitasking,
        Self::Displays,
        Self::NightLight,
        Self::Power,
        Self::ScreenShare,
        Self::Miracast,
        Self::Printers,
        Self::Keyboard,
        Self::Shortcuts,
        Self::Mouse,
        Self::Gestures,
        Self::Accessibility,
        Self::LockScreen,
        Self::Privacy,
        Self::Users,
        Self::DateTime,
        Self::Region,
        Self::Apps,
        Self::Startup,
        Self::Storage,
        Self::About,
    ];

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Bluetooth => "bluetooth",
            Self::Vpn => "vpn",
            Self::Netbird => "netbird",
            Self::Notifications => "notifications",
            Self::Sound => "sound",
            Self::Appearance => "appearance",
            Self::Wallpaper => "wallpaper",
            Self::Desktop => "desktop",
            Self::TopBar => "topbar",
            Self::Windows => "windows",
            Self::Multitasking => "multitasking",
            Self::Displays => "displays",
            Self::NightLight => "night-light",
            Self::Power => "power",
            Self::ScreenShare => "screen-sharing",
            Self::Miracast => "wireless-display",
            Self::Printers => "printers",
            Self::Keyboard => "keyboard",
            Self::Shortcuts => "shortcuts",
            Self::Mouse => "mouse",
            Self::Gestures => "gestures",
            Self::Accessibility => "accessibility",
            Self::LockScreen => "lock-screen",
            Self::Privacy => "privacy",
            Self::Users => "users",
            Self::DateTime => "datetime",
            Self::Region => "region",
            Self::Apps => "apps",
            Self::Startup => "startup",
            Self::Storage => "storage",
            Self::About => "about",
        }
    }

    #[must_use]
    pub fn title(self) -> &'static str {
        t(match self {
            Self::Network => Key::SettingsNetwork,
            Self::Bluetooth => Key::Bluetooth,
            Self::Vpn => Key::SettingsVpn,
            Self::Netbird => Key::Netbird,
            Self::Notifications => Key::SettingsNotifications,
            Self::Sound => Key::SettingsSound,
            Self::Appearance => Key::SettingsAppearance,
            Self::Wallpaper => Key::SettingsWallpaper,
            Self::Desktop => Key::SettingsDesktop,
            Self::TopBar => Key::SettingsTopBar,
            Self::Windows => Key::SettingsWindows,
            Self::Multitasking => Key::SettingsMultitasking,
            Self::Displays => Key::SettingsDisplays,
            Self::NightLight => Key::SettingsNightLight,
            Self::Power => Key::SettingsPower,
            Self::ScreenShare => Key::SettingsScreenShare,
            Self::Miracast => Key::SettingsMiracast,
            Self::Printers => Key::SettingsPrinters,
            Self::Keyboard => Key::SettingsKeyboard,
            Self::Shortcuts => Key::SettingsShortcuts,
            Self::Mouse => Key::SettingsMouse,
            Self::Gestures => Key::SettingsGestures,
            Self::Accessibility => Key::SettingsAccessibility,
            Self::LockScreen => Key::SettingsLockScreen,
            Self::Privacy => Key::SettingsPrivacy,
            Self::Users => Key::SettingsUsers,
            Self::DateTime => Key::SettingsDateTime,
            Self::Region => Key::SettingsRegion,
            Self::Apps => Key::SettingsApps,
            Self::Startup => Key::SettingsStartup,
            Self::Storage => Key::SettingsStorage,
            Self::About => Key::SettingsAbout,
        })
    }

    #[must_use]
    pub const fn keywords(self) -> &'static [&'static str] {
        match self {
            Self::Network => &["network", "wifi", "wi-fi", "wireless", "internet", "ethernet", "мережа", "інтернет"],
            Self::Bluetooth => &["bluetooth", "блютуз", "пристрої", "devices"],
            Self::Vpn => &["vpn", "wireguard", "openvpn", "tunnel", "тунель"],
            Self::Netbird => &["netbird", "mesh", "peers", "exit node", "вузли", "вихідний вузол"],
            Self::Notifications => {
                &["notifications", "do not disturb", "banners", "alerts", "сповіщення", "не турбувати"]
            }
            Self::Sound => &["sound", "audio", "volume", "speaker", "microphone", "звук", "гучність"],
            Self::Appearance => {
                &["appearance", "theme", "dark", "light", "accent", "font", "icons", "вигляд", "тема", "шрифт"]
            }
            Self::Wallpaper => &["wallpaper", "background", "picture", "шпалери", "тло", "фон"],
            Self::Desktop => &["desktop", "dock", "pins", "autohide", "стільниця", "док"],
            Self::TopBar => &["top bar", "topbar", "panel", "clock", "tray", "верхня панель", "панель", "годинник"],
            Self::Windows => &["windows", "titlebar", "shadow", "border", "buttons", "вікна"],
            Self::Multitasking => {
                &["multitasking", "workspaces", "overview", "hot corner", "snapping", "простори", "огляд"]
            }
            Self::Displays => &[
                "display",
                "displays",
                "monitor",
                "screen",
                "resolution",
                "scale",
                "дисплей",
                "екран",
                "монітор",
                "масштаб",
            ],
            Self::NightLight => &["night light", "blue light", "warm", "нічне світло", "тепліше"],
            Self::Power => &["power", "battery", "energy", "sleep", "живлення", "батарея", "енергія"],
            Self::ScreenShare => {
                &["screen sharing", "screencast", "record screen", "трансляція екрана", "запис екрана"]
            }
            Self::Miracast => {
                &["miracast", "wireless display", "mirror", "tv", "бездротовий дисплей", "дублювання", "телевізор"]
            }
            Self::Printers => &["printers", "printer", "print", "cups", "принтери", "друк"],
            Self::Keyboard => &["keyboard", "layout", "input", "compose", "клавіатура", "розкладка"],
            Self::Shortcuts => &["shortcuts", "keybindings", "hotkeys", "сполучення клавіш", "гарячі клавіші"],
            Self::Mouse => &["mouse", "touchpad", "trackpad", "pointer", "миша", "тачпад"],
            Self::Gestures => &["gestures", "swipe", "pinch", "жести"],
            Self::Accessibility => {
                &["accessibility", "large text", "contrast", "cursor size", "доступність", "спеціальні можливості"]
            }
            Self::LockScreen => {
                &["lock screen", "screen lock", "lock", "blank", "idle", "екран блокування", "блокування", "замок"]
            }
            Self::Privacy => &["privacy", "security", "camera", "clipboard", "конфіденційність", "безпека"],
            Self::Users => &["users", "account", "avatar", "користувачі", "обліковий запис"],
            Self::DateTime => &["date", "time", "time zone", "clock", "дата", "час", "часовий пояс"],
            Self::Region => &["region", "language", "locale", "formats", "регіон", "мова", "формати"],
            Self::Apps => &["apps", "applications", "default apps", "застосунки", "програми"],
            Self::Startup => &["startup", "autostart", "автозапуск"],
            Self::Storage => &["storage", "disk", "space", "drives", "сховище", "диск", "місце"],
            Self::About => &["about", "system", "version", "про систему", "система"],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Section {
    Run,
    Calculator,
    Apps,
    Windows,
    Files,
    Actions,
    Commands,
    Settings,
    Provided,
}

impl Section {
    #[must_use]
    pub fn label(self) -> &'static str {
        t(match self {
            Self::Run => Key::SectionRun,
            Self::Calculator => Key::SectionCalculator,
            Self::Apps => Key::Applications,
            Self::Windows => Key::SectionWindows,
            Self::Files => Key::SectionRecentFiles,
            Self::Actions => Key::SectionActions,
            Self::Commands => Key::SectionCommands,
            Self::Settings => Key::SectionSettings,
            Self::Provided => Key::SectionProvided,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenWindow {
    pub id: u64,
    pub title: String,
    pub app_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    Run(String),
    App(usize),
    Window { index: usize, app: Option<usize> },
    File(usize),
    Action { app: usize, action: usize },
    Calculator { expression: String, value: String },
    Command(SystemCommand),
    Settings(SettingsPanel),
    Provided { index: usize, app: Option<usize> },
}

impl Entry {
    #[must_use]
    pub const fn section(&self) -> Section {
        match self {
            Self::Run(_) => Section::Run,
            Self::App(_) => Section::Apps,
            Self::Window { .. } => Section::Windows,
            Self::File(_) => Section::Files,
            Self::Action { .. } => Section::Actions,
            Self::Calculator { .. } => Section::Calculator,
            Self::Command(_) => Section::Commands,
            Self::Settings(_) => Section::Settings,
            Self::Provided { .. } => Section::Provided,
        }
    }
}

#[must_use]
pub fn app_for_window(apps: &[DesktopApp], window: &OpenWindow) -> Option<usize> {
    let wanted = window.app_id.trim();
    if wanted.is_empty() {
        return None;
    }
    let wanted = wanted.strip_suffix(".desktop").unwrap_or(wanted);
    apps.iter().position(|app| app.id.strip_suffix(".desktop").unwrap_or(&app.id).eq_ignore_ascii_case(wanted))
}

#[must_use]
pub fn window_matches(apps: &[DesktopApp], windows: &[OpenWindow], needle: &str) -> Vec<Entry> {
    ranked(
        windows.iter().enumerate().map(|(index, window)| {
            let app = app_for_window(apps, window);
            let app_name = app.and_then(|app| apps.get(app)).map(|app| app.name.as_str());
            let title = match_rank(&window.title, needle).map(name_score);
            let owner = [Some(window.app_id.as_str()), app_name]
                .into_iter()
                .flatten()
                .filter_map(|text| match_rank(text, needle))
                .min()
                .map(keyword_score);
            ((index, app), title.max(owner))
        }),
        true,
        LIST_WINDOW_LIMIT,
    )
    .into_iter()
    .map(|(index, app)| Entry::Window { index, app })
    .collect()
}

fn phrase_score(title: &str, keywords: &[&str], needle: &str) -> Option<u32> {
    let title_rank = match_rank(title, needle).map(name_score);
    let keyword_rank = keywords.iter().filter_map(|keyword| match_rank(keyword, needle)).min().map(keyword_score);
    title_rank.max(keyword_rank)
}

fn ranked<T: Copy>(items: impl Iterator<Item = (T, Option<u32>)>, minimum_needle: bool, limit: usize) -> Vec<T> {
    if !minimum_needle {
        return Vec::new();
    }
    let mut scored: Vec<(u32, usize, T)> =
        items.enumerate().filter_map(|(order, (item, score))| Some((score?, order, item))).collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().take(limit).map(|(_, _, item)| item).collect()
}

#[must_use]
pub fn action_matches(apps: &[DesktopApp], ranked_apps: &[usize], needle: &str) -> Vec<Entry> {
    let mut candidates: Vec<(u32, (usize, usize), usize, usize)> = Vec::new();
    for (app_index, app) in apps.iter().enumerate() {
        let app_rank = ranked_apps.iter().position(|&index| index == app_index);
        for (action_index, action) in app.actions.iter().enumerate() {
            let combined = format!("{} {}", app.name, action.name);
            let score = match_rank(&action.name, needle).map(name_score).or_else(|| {
                let spaced = needle.contains(' ');
                (spaced && match_rank(&combined, needle).is_some()).then_some(SCORE_KEYWORD_PREFIX)
            });
            if let Some(score) = score {
                let tiebreak = (app_rank.unwrap_or(apps.len()), app_index);
                candidates.push((score, tiebreak, app_index, action_index));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)).then(a.3.cmp(&b.3)));
    candidates.into_iter().take(LIST_ACTION_LIMIT).map(|(_, _, app, action)| Entry::Action { app, action }).collect()
}

#[must_use]
pub fn run_entries(text: &str) -> Vec<Entry> {
    let command = text.trim();
    if command.is_empty() { Vec::new() } else { vec![Entry::Run(command.to_string())] }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Sources<'a> {
    pub windows: &'a [OpenWindow],
    pub files: &'a [super::recent::RecentFile],
}

#[must_use]
pub fn file_matches(files: &[super::recent::RecentFile], needle: &str) -> Vec<Entry> {
    ranked(
        files.iter().enumerate().map(|(index, file)| (index, match_rank(&file.name, needle).map(name_score))),
        true,
        LIST_FILE_LIMIT,
    )
    .into_iter()
    .map(Entry::File)
    .collect()
}

#[must_use]
pub fn build_entries(apps: &[DesktopApp], ranked_apps: &[usize], sources: &Sources<'_>, query: &str) -> Vec<Entry> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }
    let mut entries = Vec::new();
    if let Some(value) = super::calc::calculation(query) {
        entries.push(Entry::Calculator { expression: query.trim().to_string(), value });
    }
    entries.extend(ranked_apps.iter().take(LIST_APP_LIMIT).map(|&index| Entry::App(index)));
    let long_enough = needle.chars().count() >= 2;
    if long_enough {
        entries.extend(window_matches(apps, sources.windows, &needle));
        entries.extend(file_matches(sources.files, &needle));
        entries.extend(action_matches(apps, ranked_apps, &needle));
    }
    entries.extend(
        ranked(
            SystemCommand::ALL
                .into_iter()
                .map(|command| (command, phrase_score(command.title(), command.keywords(), &needle))),
            long_enough,
            LIST_COMMAND_LIMIT,
        )
        .into_iter()
        .map(Entry::Command),
    );
    entries.extend(
        ranked(
            SettingsPanel::ALL
                .into_iter()
                .filter(|panel| *panel != SettingsPanel::Netbird || netbird_installed())
                .map(|panel| (panel, phrase_score(panel.title(), panel.keywords(), &needle))),
            long_enough,
            LIST_SETTINGS_LIMIT,
        )
        .into_iter()
        .map(Entry::Settings),
    );
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::icon::DesktopAction;

    fn app(id: &str, name: &str) -> DesktopApp {
        DesktopApp { id: id.to_string(), name: name.to_string(), command: id.to_string(), ..DesktopApp::default() }
    }

    fn names<'a>(apps: &'a [DesktopApp], order: &[usize]) -> Vec<&'a str> {
        order.iter().map(|&i| apps[i].name.as_str()).collect()
    }

    fn all(apps: &[DesktopApp]) -> Vec<usize> {
        (0..apps.len()).collect()
    }

    fn rank(apps: &[DesktopApp], query: &str) -> Vec<usize> {
        rank_apps(apps, &all(apps), query, &LaunchHistory::default(), 0)
    }

    #[test]
    fn the_tiers_run_prefix_word_substring_keyword_comment() {
        let apps = vec![
            DesktopApp { comment: "Edit text files".into(), ..app("notes", "Notes") },
            DesktopApp { keywords: vec!["editor".into()], ..app("gedit", "Gedit") },
            app("credit", "Credit Card"),
            app("text-editor", "Text Editor"),
            app("editor", "Editor"),
        ];
        assert_eq!(
            names(&apps, &rank(&apps, "edit")),
            ["Editor", "Text Editor", "Gedit", "Credit Card", "Notes"],
            "name prefix, word prefix, then name substrings in list order, then comment"
        );
    }

    #[test]
    fn keywords_and_generic_names_outrank_comments_but_not_names() {
        let apps = vec![
            DesktopApp { comment: "A web browser".into(), ..app("lynx", "Lynx") },
            DesktopApp { generic_name: "Web Browser".into(), ..app("firefox", "Firefox") },
            app("webcam", "Webcam Viewer"),
        ];
        assert_eq!(names(&apps, &rank(&apps, "web")), ["Webcam Viewer", "Firefox", "Lynx"]);
    }

    #[test]
    fn the_binary_name_and_desktop_id_are_searchable() {
        let apps =
            vec![DesktopApp { command: "/usr/bin/nautilus --new-window".into(), ..app("org.gnome.Nautilus", "Files") }];
        assert_eq!(rank(&apps, "nautilus"), [0]);
        assert_eq!(rank(&apps, "gnome"), [0]);
    }

    #[test]
    fn matching_is_case_insensitive_and_trims_the_query() {
        let apps = vec![app("firefox", "Firefox")];
        assert_eq!(rank(&apps, "  FIRE "), [0]);
    }

    #[test]
    fn short_substrings_of_comments_are_noise() {
        let apps = vec![DesktopApp { comment: "Play music".into(), ..app("player", "Player") }];
        assert!(rank(&apps, "us").is_empty());
        assert_eq!(rank(&apps, "mus"), [0], "a word prefix of the comment counts");
    }

    #[test]
    fn frequency_reorders_within_a_tier_but_never_across_tiers() {
        let apps = vec![app("fire-a", "Fire A"), app("fire-b", "Fire B"), app("bonfire", "Bonfire")];
        let mut history = LaunchHistory::default();
        for _ in 0..50 {
            history.record("fire-b", 0);
            history.record("bonfire", 0);
        }
        let order = rank_apps(&apps, &all(&apps), "fire", &history, 0);
        assert_eq!(names(&apps, &order), ["Fire B", "Fire A", "Bonfire"]);
    }

    #[test]
    fn the_frequency_bonus_is_bounded_and_monotonic() {
        assert_eq!(frequency_bonus(0.0), 0);
        assert!(frequency_bonus(1.0) < frequency_bonus(5.0));
        let ceiling = frequency_bonus(1e9);
        assert_eq!(ceiling, FREQUENCY_BONUS_MAX);
        assert!(ceiling < SCORE_NAME_PREFIX - SCORE_NAME_WORD);
        assert!(ceiling < SCORE_NAME_WORD - SCORE_NAME_SUBSTRING);
        assert!(ceiling < SCORE_NAME_SUBSTRING - SCORE_KEYWORD_PREFIX);
    }

    #[test]
    fn frequent_apps_lead_the_empty_grid_without_duplicates() {
        let apps: Vec<DesktopApp> = ["a", "b", "c", "d"].iter().map(|n| app(n, n)).collect();
        let mut history = LaunchHistory::default();
        history.record("c", 1000);
        for _ in 0..3 {
            history.record("d", 0);
        }
        let order = frequent_first(&apps, &all(&apps), &history, 1000, FREQUENT_LIMIT);
        assert_eq!(names(&apps, &order), ["d", "c", "a", "b"]);
    }

    #[test]
    fn recency_decay_lets_a_recent_app_overtake_an_old_favourite() {
        let apps: Vec<DesktopApp> = ["old", "new"].iter().map(|n| app(n, n)).collect();
        let now = 200 * 86_400;
        let mut history = LaunchHistory::default();
        for _ in 0..10 {
            history.record("old", 0);
        }
        for _ in 0..2 {
            history.record("new", now);
        }
        let order = frequent_first(&apps, &all(&apps), &history, now, FREQUENT_LIMIT);
        assert_eq!(names(&apps, &order), ["new", "old"]);
    }

    #[test]
    fn the_frequent_row_is_capped() {
        let apps: Vec<DesktopApp> = (0..12).map(|i| app(&i.to_string(), "x")).collect();
        let mut history = LaunchHistory::default();
        for a in &apps {
            history.record(&a.id, 0);
        }
        let order = frequent_first(&apps, &all(&apps), &history, 0, 3);
        assert_eq!(order.len(), 12, "nothing is dropped");
        assert_eq!(&order[..3], [0, 1, 2]);
    }

    fn firefox() -> DesktopApp {
        DesktopApp {
            actions: vec![
                DesktopAction {
                    id: "new-window".into(),
                    name: "New Window".into(),
                    command: "firefox --new-window".into(),
                },
                DesktopAction {
                    id: "private".into(),
                    name: "New Private Window".into(),
                    command: "firefox --private-window".into(),
                },
            ],
            ..app("firefox", "Firefox")
        }
    }

    #[test]
    fn desktop_actions_match_on_their_own_name() {
        let apps = vec![firefox()];
        let entries = build_entries(&apps, &[], &Sources::default(), "private");
        assert!(entries.contains(&Entry::Action { app: 0, action: 1 }));
        assert!(!entries.contains(&Entry::Action { app: 0, action: 0 }));
    }

    #[test]
    fn a_spaced_query_can_name_the_app_and_the_action() {
        let apps = vec![firefox()];
        let entries = build_entries(&apps, &[0], &Sources::default(), "firefox new private");
        assert!(entries.contains(&Entry::Action { app: 0, action: 1 }));
    }

    fn window(id: u64, title: &str, app_id: &str) -> OpenWindow {
        OpenWindow { id, title: title.to_string(), app_id: app_id.to_string() }
    }

    #[test]
    fn open_windows_are_found_by_title_and_by_their_app() {
        let apps = vec![firefox(), app("org.gnome.TextEditor", "Text Editor")];
        let windows = vec![
            window(7, "Quarterly report — Mozilla Firefox", "firefox"),
            window(9, "notes.txt", "org.gnome.TextEditor"),
            window(11, "Untitled", ""),
        ];
        let found = |query| {
            build_entries(&apps, &[], &Sources { windows: &windows, files: &[] }, query)
                .into_iter()
                .filter(|entry| entry.section() == Section::Windows)
                .collect::<Vec<_>>()
        };
        assert_eq!(found("report"), [Entry::Window { index: 0, app: Some(0) }]);
        assert_eq!(found("text"), [Entry::Window { index: 1, app: Some(1) }]);
        assert!(found("u").is_empty(), "one letter is too little to search windows");
    }

    #[test]
    fn the_run_prompt_offers_exactly_the_typed_command() {
        assert_eq!(run_entries("  firefox --private-window "), [Entry::Run("firefox --private-window".into())]);
        assert!(run_entries("   ").is_empty());
        assert_eq!(Entry::Run("ls".into()).section(), Section::Run);
    }

    #[test]
    fn recent_files_are_found_by_name_in_recency_order() {
        let file = |name: &str| super::super::recent::RecentFile {
            path: std::path::PathBuf::from(format!("/home/user/{name}")),
            name: name.to_string(),
            folder: "~".to_string(),
        };
        let files = vec![file("budget.ods"), file("notes.txt"), file("old budget.ods")];
        let sources = Sources { windows: &[], files: &files };
        let found: Vec<Entry> = build_entries(&[], &[], &sources, "budget")
            .into_iter()
            .filter(|entry| entry.section() == Section::Files)
            .collect();
        assert_eq!(found, [Entry::File(0), Entry::File(2)]);
        assert!(file_matches(&files, "zzz").is_empty());
    }

    #[test]
    fn a_title_match_outranks_an_app_match() {
        let apps = vec![firefox()];
        let windows = vec![window(1, "Docs", "firefox"), window(2, "Firefox release notes", "org.gnome.Epiphany")];
        let entries = window_matches(&apps, &windows, "firefox");
        assert_eq!(entries, [Entry::Window { index: 1, app: None }, Entry::Window { index: 0, app: Some(0) }]);
    }

    #[test]
    fn window_app_ids_match_desktop_ids_loosely() {
        let apps = vec![app("Alacritty", "Alacritty"), app("org.gnome.Nautilus.desktop", "Files")];
        assert_eq!(app_for_window(&apps, &window(1, "", "alacritty")), Some(0));
        assert_eq!(app_for_window(&apps, &window(1, "", "org.gnome.Nautilus")), Some(1));
        assert_eq!(app_for_window(&apps, &window(1, "", " ")), None);
    }

    #[test]
    fn a_math_query_puts_the_calculator_first() {
        let entries = build_entries(&[], &[], &Sources::default(), "2+2");
        assert_eq!(entries.first(), Some(&Entry::Calculator { expression: "2+2".into(), value: "4".into() }));
    }

    #[test]
    fn commands_match_in_either_language() {
        for query in ["reboot", "перезав", "restart"] {
            let entries = build_entries(&[], &[], &Sources::default(), query);
            assert!(entries.contains(&Entry::Command(SystemCommand::Restart)), "{query}: {entries:?}");
        }
        let entries = build_entries(&[], &[], &Sources::default(), "lock");
        assert_eq!(entries.first(), Some(&Entry::Command(SystemCommand::Lock)));
    }

    #[test]
    fn settings_panels_match_by_keyword() {
        let entries = build_entries(&[], &[], &Sources::default(), "wifi");
        assert!(entries.contains(&Entry::Settings(SettingsPanel::Network)));
        let entries = build_entries(&[], &[], &Sources::default(), "гучн");
        assert!(entries.contains(&Entry::Settings(SettingsPanel::Sound)));
    }

    #[test]
    fn sections_come_out_grouped_in_display_order() {
        let apps = vec![app("power-stats", "Power Statistics")];
        let entries = build_entries(&apps, &[0], &Sources::default(), "power");
        let sections: Vec<Section> = entries.iter().map(Entry::section).collect();
        let mut sorted = sections.clone();
        sorted.sort();
        assert_eq!(sections, sorted, "{entries:?}");
        assert!(sections.contains(&Section::Apps));
        assert!(sections.contains(&Section::Commands));
        assert!(sections.contains(&Section::Settings));
    }

    #[test]
    fn single_letters_only_search_apps() {
        let apps = vec![app("l", "Lollypop")];
        let entries = build_entries(&apps, &[0], &Sources::default(), "l");
        assert_eq!(entries, [Entry::App(0)]);
    }

    #[test]
    fn the_settings_slugs_are_ones_glide_settings_accepts() {
        let slugs: Vec<&str> = SettingsPanel::ALL.iter().map(|p| p.slug()).collect();
        assert_eq!(
            slugs,
            [
                "network",
                "bluetooth",
                "vpn",
                "netbird",
                "notifications",
                "sound",
                "appearance",
                "wallpaper",
                "desktop",
                "topbar",
                "windows",
                "multitasking",
                "displays",
                "night-light",
                "power",
                "screen-sharing",
                "wireless-display",
                "printers",
                "keyboard",
                "shortcuts",
                "mouse",
                "gestures",
                "accessibility",
                "lock-screen",
                "privacy",
                "users",
                "datetime",
                "region",
                "apps",
                "startup",
                "storage",
                "about"
            ],
            "one per glide-settings panel, in its sidebar order"
        );
    }

    #[test]
    fn a_setting_is_found_under_its_own_panel() {
        for (query, panel) in [
            ("wallpaper", SettingsPanel::Wallpaper),
            ("night light", SettingsPanel::NightLight),
            ("vpn", SettingsPanel::Vpn),
            ("shortcuts", SettingsPanel::Shortcuts),
            ("autostart", SettingsPanel::Startup),
            ("printers", SettingsPanel::Printers),
            ("privacy", SettingsPanel::Privacy),
        ] {
            let entries = build_entries(&[], &[], &Sources::default(), query);
            let first = entries.iter().find_map(|entry| match entry {
                Entry::Settings(panel) => Some(*panel),
                _ => None,
            });
            assert_eq!(first, Some(panel), "{query}");
        }
    }
}
