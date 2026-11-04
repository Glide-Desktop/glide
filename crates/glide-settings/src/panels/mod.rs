pub mod about;
pub mod accessibility;
pub mod appearance;
pub mod apps;
pub mod bluetooth;
pub mod common;
pub mod date_time;
pub mod desktop;
pub mod displays;
pub mod gestures;
pub mod home;
pub mod keyboard;
pub mod lock_screen;
pub mod miracast;
pub mod mouse;
pub mod multitasking;
pub mod netbird;
pub mod network;
pub mod night_light;
pub mod notifications;
pub mod passwords;
pub mod power;
pub mod printers;
pub mod privacy;
pub mod region;
pub mod screen_share;
pub mod search;
pub mod shortcuts;
pub mod sound;
pub mod startup;
pub mod storage;
pub mod topbar;
pub mod users;
pub mod vpn;
pub mod wallpaper;
pub mod windows;

use iced::{Color, Element, Subscription, Task};

use crate::i18n::{Key, Lang, Text};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::theme::{Accent, ThemeState, ramp_color, to_iced_color};

#[derive(Debug, Clone, Copy)]
pub struct Ctx {
    pub theme: ThemeState,
    pub lang: Lang,
    pub writable: bool,
    pub online: bool,
}

impl Ctx {
    #[must_use]
    pub fn t<K: Text>(self, key: K) -> &'static str {
        key.text(self.lang)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageEvent {
    Reloaded,
    OutputsChanged,
}

pub trait Pane: Default {
    type Msg: std::fmt::Debug + Clone + Send + 'static;

    const KEYWORDS: &'static [&'static str] = &[];
    const POLLS: bool = false;

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Self::Msg> {
        Task::none()
    }

    fn leave(&mut self, _config: &mut ConfigStore) {}

    fn update(&mut self, msg: Self::Msg, c: Ctx, config: &mut ConfigStore) -> Task<Self::Msg>;

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Self::Msg>;

    fn tick(&mut self, _c: Ctx) -> Task<Self::Msg> {
        Task::none()
    }

    fn event(&mut self, _event: PageEvent, _c: Ctx) -> Task<Self::Msg> {
        Task::none()
    }

    fn sidebar_status(&self, _c: Ctx) -> Option<String> {
        None
    }

    fn subscription(&self) -> Subscription<Self::Msg> {
        Subscription::none()
    }
}

macro_rules! with_panels {
    ($callback:ident) => {
        $callback! {
            Home => home,
            Network => network,
            Bluetooth => bluetooth,
            Vpn => vpn,
            Netbird => netbird,
            Notifications => notifications,
            Sound => sound,
            Appearance => appearance,
            Wallpaper => wallpaper,
            Desktop => desktop,
            TopBar => topbar,
            Windows => windows,
            Multitasking => multitasking,
            Displays => displays,
            NightLight => night_light,
            Power => power,
            ScreenShare => screen_share,
            Miracast => miracast,
            Keyboard => keyboard,
            Shortcuts => shortcuts,
            Mouse => mouse,
            Gestures => gestures,
            Accessibility => accessibility,
            LockScreen => lock_screen,
            Privacy => privacy,
            Passwords => passwords,
            Users => users,
            DateTime => date_time,
            Region => region,
            Search => search,
            Apps => apps,
            Startup => startup,
            Storage => storage,
            Printers => printers,
            About => about,
        }
    };
}

pub(crate) use with_panels;

macro_rules! declare_panels {
    ($($variant:ident => $module:ident,)*) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Panel {
            $($variant,)*
        }

        impl Panel {
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            #[must_use]
            pub const fn keywords(self) -> &'static [&'static str] {
                match self {
                    $(Self::$variant => <$module::Page as Pane>::KEYWORDS,)*
                }
            }

            #[must_use]
            pub(crate) const fn polls(self) -> bool {
                match self {
                    $(Self::$variant => <$module::Page as Pane>::POLLS,)*
                }
            }
        }
    };
}

with_panels!(declare_panels);

macro_rules! panel_meta {
    ($($variant:ident: $key:ident, $about:ident, $slug:literal, $icon:ident, $accent:ident $step:literal;)*) => {
        impl Panel {
            #[must_use]
            pub const fn key(self) -> Key {
                match self {
                    $(Self::$variant => Key::$key,)*
                }
            }

            #[must_use]
            pub(crate) const fn description_key(self) -> Key {
                match self {
                    $(Self::$variant => Key::$about,)*
                }
            }

            #[must_use]
            pub const fn slug(self) -> &'static str {
                match self {
                    $(Self::$variant => $slug,)*
                }
            }

            #[must_use]
            pub const fn icon(self) -> Icon {
                match self {
                    $(Self::$variant => Icon::$icon,)*
                }
            }

            #[must_use]
            pub fn color(self) -> Color {
                match self {
                    $(Self::$variant => ramp_color(Accent::$accent, $step),)*
                }
            }
        }
    };
}

panel_meta! {
    Home: PanelHome, AboutHome, "home", Home, Graphite 2;
    Network: PanelNetwork, AboutNetwork, "network", Wifi, Azurit 3;
    Bluetooth: PanelBluetooth, AboutBluetooth, "bluetooth", Bluetooth, Azurit 4;
    Vpn: PanelVpn, AboutVpn, "vpn", Shield, Azurit 5;
    Netbird: PanelNetbird, AboutNetbird, "netbird", Link, Amber 2;
    Notifications: PanelNotifications, AboutNotifications, "notifications", Bell, Cinnabar 4;
    Sound: PanelSound, AboutSound, "sound", Volume, Cinnabar 3;
    Appearance: PanelAppearance, AboutAppearance, "appearance", Appearance, Amethyst 3;
    Wallpaper: PanelWallpaper, AboutWallpaper, "wallpaper", Image, Azurit 2;
    Desktop: PanelDesktop, AboutDesktop, "dock", Dock, Amethyst 4;
    TopBar: PanelTopBar, AboutTopBar, "topbar", TopBar, Amethyst 5;
    Windows: PanelWindows, AboutWindows, "windows", Window, Malachite 4;
    Multitasking: PanelMultitasking, AboutMultitasking, "multitasking", Grid, Malachite 5;
    Displays: PanelDisplays, AboutDisplays, "displays", Display, Malachite 3;
    NightLight: PanelNightLight, AboutNightLight, "night-light", Moon, Amber 4;
    Power: PanelPower, AboutPower, "power", Battery, Conifer 3;
    ScreenShare: PanelScreenShare, AboutScreenShare, "screen-sharing", Cast, Amethyst 5;
    Miracast: PanelMiracast, AboutMiracast, "wireless-display", Tv, Terracotta 2;
    Keyboard: PanelKeyboard, AboutKeyboard, "keyboard", Keyboard, Graphite 3;
    Shortcuts: PanelShortcuts, AboutShortcuts, "shortcuts", Command, Graphite 5;
    Mouse: PanelMouse, AboutMouse, "mouse", Mouse, Terracotta 3;
    Gestures: PanelGestures, AboutGestures, "gestures", Gesture, Terracotta 2;
    Accessibility: PanelAccessibility, AboutAccessibility, "accessibility", Accessibility, Malachite 2;
    LockScreen: PanelLockScreen, AboutLockScreen, "lock-screen", Lock, Cinnabar 2;
    Privacy: PanelPrivacy, AboutPrivacy, "privacy", Hand, Amethyst 2;
    Passwords: PanelPasswords, AboutPasswords, "passwords", Key, Amber 4;
    Users: PanelUsers, AboutUsers, "users", Users, Terracotta 4;
    DateTime: PanelDateTime, AboutDateTime, "datetime", Clock, Cinnabar 5;
    Region: PanelRegion, AboutRegion, "region", Globe, Conifer 4;
    Search: PanelSearch, AboutSearch, "search", Search, Conifer 2;
    Apps: PanelApps, AboutApps, "default-apps", Apps, Amber 3;
    Startup: PanelStartup, AboutStartup, "startup", Rocket, Terracotta 5;
    Storage: PanelStorage, AboutStorage, "storage", Drive, Conifer 5;
    Printers: PanelPrinters, AboutPrinters, "printers", Printer, Graphite 4;
    About: PanelAbout, AboutSystem, "about", Info, Amber 5;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Home,
    Network,
    Bluetooth,
    Vpn,
    Notifications,
    Sound,
    Appearance,
    Desktop,
    Displays,
    Power,
    ScreenShare,
    Miracast,
    Printers,
    Keyboard,
    Mouse,
    Search,
    Apps,
    Startup,
    LockScreen,
    Privacy,
    Passwords,
    Accessibility,
    Users,
    DateTime,
    Region,
    Storage,
    About,
}

#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub title: Option<Key>,
    pub categories: &'static [Category],
}

impl Category {
    pub const ALL: &'static [Self] = &[
        Self::Home,
        Self::Network,
        Self::Bluetooth,
        Self::Vpn,
        Self::Notifications,
        Self::Sound,
        Self::Appearance,
        Self::Desktop,
        Self::Displays,
        Self::Power,
        Self::ScreenShare,
        Self::Miracast,
        Self::Printers,
        Self::Keyboard,
        Self::Mouse,
        Self::Search,
        Self::Apps,
        Self::Startup,
        Self::LockScreen,
        Self::Privacy,
        Self::Passwords,
        Self::Accessibility,
        Self::Users,
        Self::DateTime,
        Self::Region,
        Self::Storage,
        Self::About,
    ];

    pub const GROUPS: &'static [Group] = &[
        Group { title: None, categories: &[Self::Home] },
        Group { title: Some(Key::GroupConnections), categories: &[Self::Network, Self::Bluetooth, Self::Vpn] },
        Group { title: Some(Key::GroupAlerts), categories: &[Self::Notifications, Self::Sound] },
        Group { title: Some(Key::GroupPersonalisation), categories: &[Self::Appearance, Self::Desktop] },
        Group {
            title: Some(Key::GroupHardware),
            categories: &[Self::Displays, Self::Power, Self::ScreenShare, Self::Miracast, Self::Printers],
        },
        Group { title: Some(Key::GroupInput), categories: &[Self::Keyboard, Self::Mouse] },
        Group { title: Some(Key::GroupApps), categories: &[Self::Search, Self::Apps, Self::Startup] },
        Group {
            title: Some(Key::GroupPeople),
            categories: &[Self::LockScreen, Self::Privacy, Self::Passwords, Self::Accessibility, Self::Users],
        },
        Group {
            title: Some(Key::GroupSystem),
            categories: &[Self::DateTime, Self::Region, Self::Storage, Self::About],
        },
    ];

    #[must_use]
    pub const fn panels(self) -> &'static [Panel] {
        match self {
            Self::Home => &[Panel::Home],
            Self::Network => &[Panel::Network],
            Self::Bluetooth => &[Panel::Bluetooth],
            Self::Vpn => &[Panel::Vpn, Panel::Netbird],
            Self::Notifications => &[Panel::Notifications],
            Self::Sound => &[Panel::Sound],
            Self::Appearance => &[Panel::Appearance, Panel::Wallpaper],
            Self::Desktop => &[Panel::Desktop, Panel::TopBar, Panel::Windows, Panel::Multitasking],
            Self::Displays => &[Panel::Displays, Panel::NightLight],
            Self::Power => &[Panel::Power],
            Self::ScreenShare => &[Panel::ScreenShare],
            Self::Miracast => &[Panel::Miracast],
            Self::Printers => &[Panel::Printers],
            Self::Keyboard => &[Panel::Keyboard, Panel::Shortcuts],
            Self::Mouse => &[Panel::Mouse, Panel::Gestures],
            Self::LockScreen => &[Panel::LockScreen],
            Self::Search => &[Panel::Search],
            Self::Apps => &[Panel::Apps],
            Self::Startup => &[Panel::Startup],
            Self::Privacy => &[Panel::Privacy],
            Self::Passwords => &[Panel::Passwords],
            Self::Accessibility => &[Panel::Accessibility],
            Self::Users => &[Panel::Users],
            Self::DateTime => &[Panel::DateTime],
            Self::Region => &[Panel::Region],
            Self::Storage => &[Panel::Storage],
            Self::About => &[Panel::About],
        }
    }

    #[must_use]
    pub const fn stacked(self) -> bool {
        self.panels().len() > 1
    }

    #[must_use]
    pub const fn first(self) -> Panel {
        self.panels()[0]
    }

    #[must_use]
    pub fn of(panel: Panel) -> Self {
        Self::ALL.iter().copied().find(|category| category.panels().contains(&panel)).unwrap_or(Self::About)
    }

    #[must_use]
    pub fn group(self) -> &'static Group {
        Self::GROUPS
            .iter()
            .find(|group| group.categories.contains(&self))
            .unwrap_or(&Self::GROUPS[Self::GROUPS.len() - 1])
    }

    #[must_use]
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            _ => self.first().slug(),
        }
    }

    #[must_use]
    pub const fn label(self, lang: Lang) -> &'static str {
        let meta = self.meta();
        match lang {
            Lang::Uk => meta.uk,
            Lang::En => meta.en,
        }
    }

    #[must_use]
    pub const fn description(self, lang: Lang) -> &'static str {
        let meta = self.meta();
        match lang {
            Lang::Uk => meta.about_uk,
            Lang::En => meta.about_en,
        }
    }

    #[must_use]
    pub const fn icon(self) -> crate::icons::Tile {
        crate::icons::Tile(self.meta().glyph)
    }

    #[must_use]
    pub fn color(self) -> Color {
        let meta = self.meta();
        to_iced_color(meta.accent.ramp()[meta.step])
    }

    #[must_use]
    pub fn ink(self) -> Color {
        let meta = self.meta();
        if meta.dark { to_iced_color(meta.accent.ramp()[5]) } else { Color::WHITE }
    }

    #[must_use]
    pub fn matches(self, query: &str) -> bool {
        self.panels().iter().any(|panel| panel.matches(query))
            || [Lang::Uk, Lang::En]
                .iter()
                .any(|lang| self.label(*lang).to_lowercase().contains(&query.trim().to_lowercase()))
    }

    #[must_use]
    pub fn matched_panel(self, query: &str) -> Option<Panel> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return None;
        }
        self.panels().iter().copied().find(|panel| panel.label_matches(&query)).or_else(|| {
            self.panels().iter().copied().find(|panel| panel.keywords().iter().any(|word| word.contains(&query)))
        })
    }
}

impl Panel {
    fn label_matches(self, query: &str) -> bool {
        [Lang::Uk, Lang::En].iter().any(|lang| self.label(*lang).to_lowercase().contains(query))
    }

    #[must_use]
    pub fn matched_keyword(self, query: &str) -> Option<&'static str> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || self.label_matches(&query) {
            return None;
        }
        self.keywords().iter().copied().find(|word| word.contains(&query))
    }

    #[must_use]
    pub const fn label(self, lang: Lang) -> &'static str {
        self.key().text(lang)
    }

    #[must_use]
    pub const fn description(self, lang: Lang) -> &'static str {
        self.description_key().text(lang)
    }

    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        let slug = slug.trim().to_ascii_lowercase();
        let alias = match slug.as_str() {
            "wifi" | "wi-fi" => Some(Self::Network),
            "display" => Some(Self::Displays),
            "touchpad" => Some(Self::Mouse),
            "desktop" => Some(Self::Desktop),
            "apps" => Some(Self::Apps),
            "battery" => Some(Self::Power),
            "date" | "time" => Some(Self::DateTime),
            "language" => Some(Self::Region),
            "a11y" => Some(Self::Accessibility),
            "lock" | "screen-lock" | "lockscreen" => Some(Self::LockScreen),
            "security" => Some(Self::Privacy),
            "autostart" => Some(Self::Startup),
            "disk" => Some(Self::Storage),
            "keybinds" => Some(Self::Shortcuts),
            "launcher" => Some(Self::Search),
            "workspaces" | "overview" => Some(Self::Multitasking),
            "gesture" | "swipe" => Some(Self::Gestures),
            _ => None,
        };
        alias.or_else(|| {
            Self::ALL
                .iter()
                .copied()
                .find(|panel| panel.slug() == slug || (slug.len() >= 3 && panel.slug().starts_with(&slug)))
        })
    }

    #[must_use]
    pub fn matches(self, query: &str) -> bool {
        let query = query.trim().to_lowercase();
        query.is_empty() || self.label_matches(&query) || self.keywords().iter().any(|word| word.contains(&query))
    }
}

pub struct Meta {
    en: &'static str,
    uk: &'static str,
    about_en: &'static str,
    about_uk: &'static str,
    accent: glide_sdk::theme::Accent,
    step: usize,
    dark: bool,
    glyph: &'static str,
}

macro_rules! category_meta {
    ($($category:ident: $en:literal, $uk:literal, $about_en:literal, $about_uk:literal, $accent:ident $step:literal $dark:literal, $glyph:expr;)*) => {
        impl Category {
            #[must_use]
            pub const fn meta(self) -> &'static Meta {
                match self {
                    $(Self::$category => &Meta {
                        en: $en,
                        uk: $uk,
                        about_en: $about_en,
                        about_uk: $about_uk,
                        accent: glide_sdk::theme::Accent::$accent,
                        step: $step,
                        dark: $dark,
                        glyph: $glyph,
                    },)*
                }
            }
        }
    };
}

use crate::icons::tile_svg;

category_meta! {
    Home: "Overview", "Огляд", "This computer at a glance. Every tile leads to its section.", "Стан комп’ютера одним поглядом. Кожна плитка веде до свого розділу.", Blue 3 false,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M3 11l9-8 9 8v10h-6v-6H9v6H3z'/>");
    Network: "Network", "Мережа", "Wi-Fi, wired connection, hotspot and proxy.", "Wi-Fi, дротове підключення, точка доступу й проксі.", Blue 2 false,
        tile_svg!("2.4", "stroke-linecap='round'", "<path d='M2 9a15 15 0 0 1 20 0M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0M12 19.5h.01'/>");
    Bluetooth: "Bluetooth", "Bluetooth", "Headphones, mice, keyboards and phones nearby.", "Навушники, миші, клавіатури й телефони поруч.", Blue 3 false,
        tile_svg!("2.4", "stroke-linecap='round' stroke-linejoin='round'", "<path d='M7 7l10 10-5 5V2l5 5L7 17'/>");
    Vpn: "VPN", "VPN", "NetworkManager tunnels and the NetBird mesh in one place.", "Захищені тунелі NetworkManager і mesh-мережа NetBird в одному місці.", Blue 4 false,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z'/>");
    Notifications: "Notifications", "Сповіщення", "Banners in the island at the top of the screen, sounds and Do Not Disturb.", "Банери в острові вгорі екрана, звуки й режим «Не турбувати».", Red 2 false,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M6 16v-5a6 6 0 0 1 12 0v5l2 2H4zM10 21h4'/>");
    Sound: "Sound", "Звук", "Where sound goes, where the microphone comes from and each app’s volume.", "Куди йде звук, звідки береться мікрофон і гучність кожної програми.", Red 1 true,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M4 9h4l5-4v14l-5-4H4zM16 9a4 4 0 0 1 0 6'/>");
    Appearance: "Appearance", "Вигляд", "Theme, accent, wallpaper, fonts and motion. Applies to the shell, Glide apps, GTK and Qt.", "Тема, акцент, шпалери, шрифти й рух. Діє на оболонку, програми Glide, GTK і Qt.", Purple 2 false,
        tile_svg!("2.2", "", "<circle cx='12' cy='12' r='8.5'/><path d='M12 3.5a8.5 8.5 0 0 0 0 17z' fill='black'/>");
    Desktop: "Desktop", "Робочий стіл", "Dock, top bar, windows, workspaces and the overview.", "Док, верхня панель, вікна, робочі простори й огляд.", Purple 3 false,
        tile_svg!("2.2", "", "<rect x='3' y='3' width='18' height='13' rx='2'/><path d='M3 7.5h18M8 20h8'/>");
    Displays: "Displays", "Дисплеї", "Arrangement, resolution, scale and night light. A mode change reverts by itself after 15 s unless you keep it.", "Розташування, роздільність, масштаб і нічне світло. Зміна режиму сама повертається через 15 с, якщо її не підтвердити.", Teal 3 false,
        tile_svg!("2.2", "", "<rect x='3' y='4' width='18' height='12' rx='2'/><path d='M8 20h8M12 16v4'/>");
    Power: "Power & Battery", "Живлення й батарея", "Power mode, sleep when idle, charging, the lid and the power button.", "Режим живлення, сон за бездіяльності, заряджання, кришка й кнопка живлення.", Green 3 false,
        tile_svg!("2.2", "", "<rect x='2.5' y='7' width='17' height='10' rx='2'/><path d='M22 11v2'/>");
    ScreenShare: "Screen Sharing", "Трансляція екрана", "Who sees your screen: sharing in calls, screen recording, third-party screenshot apps.", "Хто бачить ваш екран: демонстрація в дзвінках, запис відео, сторонні програми знімків.", Orange 3 false,
        tile_svg!("2.2", "stroke-linecap='round' stroke-linejoin='round'", "<rect x='3' y='4' width='18' height='13' rx='2'/><path d='M12 14V8M9 10.5l3-3 3 3M8 21h8'/>");
    Miracast: "Wireless Display", "Бездротовий дисплей", "Show the screen on a TV or projector with Miracast, with no cable and no shared network.", "Показати екран на телевізорі чи проєкторі з Miracast, без кабелю й без спільної мережі.", Orange 2 false,
        tile_svg!("2.2", "stroke-linecap='round' stroke-linejoin='round'", "<rect x='2' y='7' width='20' height='13' rx='2'/><path d='M8 3l4 4 4-4'/>");
    Printers: "Printers", "Принтери", "Printers, network scanners and the print queue.", "Принтери, сканери в мережі та черга друку.", Graphite 4 false,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M6 9V3h12v6M6 17H3V9h18v8h-3M7 14h10v7H7z'/>");
    Keyboard: "Keyboard", "Клавіатура", "Layouts, special keys, key repeat and keyboard shortcuts.", "Розкладки, спеціальні клавіші, повтор і комбінації клавіш.", Graphite 3 false,
        tile_svg!("2.2", "stroke-linecap='round'", "<rect x='2' y='6' width='20' height='12' rx='2'/><path d='M6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10'/>");
    Mouse: "Mouse & Touchpad", "Миша й тачпад", "Speed, scrolling, tapping and multi-finger gestures.", "Швидкість, прокручування, дотики й жести кількома пальцями.", Orange 3 false,
        tile_svg!("2.2", "", "<rect x='6' y='3' width='12' height='18' rx='6'/><path d='M12 7v3'/>");
    Search: "Search & Launcher", "Пошук і лаунчер", "What the launcher finds, in what order, and how to open it.", "Що знаходить лаунчер, у якому порядку, і як його відкрити.", Blue 3 false,
        tile_svg!("2.4", "stroke-linecap='round'", "<circle cx='11' cy='11' r='6'/><path d='M20 20l-4.5-4.5'/>");
    Apps: "Default Apps", "Типові програми", "What opens links, mail, files and the terminal.", "Чим відкриваються посилання, пошта, файли й термінал.", Amber 2 true,
        tile_svg!("0", "", "<g fill='black' stroke='none'><circle cx='7.5' cy='7.5' r='3'/><circle cx='16.5' cy='7.5' r='3'/><circle cx='7.5' cy='16.5' r='3'/><circle cx='16.5' cy='16.5' r='3'/></g>");
    Startup: "Startup Apps", "Автозапуск", "What opens by itself when you sign in.", "Що відкривається само, коли ви входите в систему.", Orange 4 false,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M5 15c-1 1-1 4-1 4s3 0 4-1M9 15l-3-3c2-5 7-9 12-9 0 5-4 10-9 12z'/>");
    LockScreen: "Lock Screen", "Екран блокування", "When the session locks and what the lock screen shows.", "Коли сеанс блокується і що видно на заблокованому екрані.", Purple 5 false,
        tile_svg!("2.2", "", "<rect x='5' y='11' width='14' height='10' rx='2'/><path d='M8 11V7a4 4 0 0 1 8 0v4'/>");
    Privacy: "Privacy", "Конфіденційність", "What apps see and what stays on disk after you.", "Що програми бачать і що лишається на диску після вас.", Purple 1 true,
        tile_svg!("2.2", "", "<path d='M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z'/><circle cx='12' cy='12' r='3'/>");
    Passwords: "Passwords & Autofill", "Паролі й автозаповнення", "How Glide offers saved passwords and codes and how it protects the vaults.", "Як Glide пропонує збережені паролі й коди та як захищає сховища.", Amber 4 false,
        tile_svg!("2.2", "stroke-linecap='round'", "<circle cx='8' cy='15' r='4'/><path d='M11 12l9-9M17 6l3 3'/>");
    Accessibility: "Accessibility", "Спеціальні можливості", "Easier to see, point and read. GTK apps follow these settings too.", "Легше бачити, вказувати й читати. GTK-програми теж ці налаштування підхоплюють.", Green 2 false,
        tile_svg!("2.2", "stroke-linecap='round'", "<circle cx='12' cy='4.5' r='1.5'/><path d='M5 8.5h14M12 8.5v5M8.5 21l3.5-7.5 3.5 7.5'/>");
    Users: "Users", "Користувачі", "Your account and the other people on this computer.", "Ваш обліковий запис і інші люди на цьому комп’ютері.", Red 3 false,
        tile_svg!("2.2", "stroke-linecap='round'", "<circle cx='9' cy='8' r='4'/><path d='M2 21a7 7 0 0 1 14 0M17 4a4 4 0 0 1 0 8M22 21a6 6 0 0 0-4-5.6'/>");
    DateTime: "Date & Time", "Дата й час", "Time zone, synchronisation and how the clock looks.", "Часовий пояс, синхронізація і вигляд годинника.", Red 4 false,
        tile_svg!("2.2", "stroke-linecap='round'", "<circle cx='12' cy='12' r='8.5'/><path d='M12 7v5l3 2'/>");
    Region: "Region & Language", "Регіон і мова", "The interface language and the formats of dates, numbers and money.", "Мова інтерфейсу та формати дат, чисел і грошей.", Green 4 false,
        tile_svg!("2.2", "", "<circle cx='12' cy='12' r='8.5'/><path d='M3.5 12h17M12 3.5a13 13 0 0 1 0 17M12 3.5a13 13 0 0 0 0 17'/>");
    Storage: "Storage", "Сховище", "What takes up disk space and what can be freed.", "Що займає місце на диску і що можна звільнити.", Green 5 false,
        tile_svg!("2.2", "stroke-linejoin='round'", "<path d='M3 13l3-8h12l3 8v6H3zM3 13h18M7 16.5h.01'/>");
    About: "About", "Про систему", "This computer and the software on it.", "Цей комп’ютер і програмне забезпечення на ньому.", Amber 5 false,
        tile_svg!("2.2", "stroke-linecap='round'", "<circle cx='12' cy='12' r='8.5'/><path d='M12 11v6M12 7.5h.01'/>");
}
