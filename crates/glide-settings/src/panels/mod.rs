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
pub mod keyboard;
pub mod mouse;
pub mod multitasking;
pub mod netbird;
pub mod network;
pub mod night_light;
pub mod notifications;
pub mod power;
pub mod printers;
pub mod privacy;
pub mod region;
pub mod shortcuts;
pub mod sound;
pub mod startup;
pub mod storage;
pub mod terminal;
pub mod topbar;
pub mod users;
pub mod vpn;
pub mod wallpaper;
pub mod windows;

use iced::{Color, Element, Subscription, Task};

use crate::i18n::{Key, Lang, Text};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::theme::{Accent, ThemeState, ramp_color};

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
            Keyboard => keyboard,
            Shortcuts => shortcuts,
            Mouse => mouse,
            Gestures => gestures,
            Accessibility => accessibility,
            Privacy => privacy,
            Users => users,
            DateTime => date_time,
            Region => region,
            Apps => apps,
            Terminal => terminal,
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
            pub const fn polls(self) -> bool {
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
            pub const fn description_key(self) -> Key {
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
    Keyboard: PanelKeyboard, AboutKeyboard, "keyboard", Keyboard, Graphite 3;
    Shortcuts: PanelShortcuts, AboutShortcuts, "shortcuts", Command, Graphite 5;
    Mouse: PanelMouse, AboutMouse, "mouse", Mouse, Terracotta 3;
    Gestures: PanelGestures, AboutGestures, "gestures", Gesture, Terracotta 2;
    Accessibility: PanelAccessibility, AboutAccessibility, "accessibility", Accessibility, Malachite 2;
    Privacy: PanelPrivacy, AboutPrivacy, "privacy", Hand, Amethyst 2;
    Users: PanelUsers, AboutUsers, "users", Users, Terracotta 4;
    DateTime: PanelDateTime, AboutDateTime, "datetime", Clock, Cinnabar 5;
    Region: PanelRegion, AboutRegion, "region", Globe, Conifer 4;
    Apps: PanelApps, AboutApps, "default-apps", Apps, Amber 3;
    Terminal: PanelTerminal, AboutTerminal, "terminal", Terminal, Graphite 2;
    Startup: PanelStartup, AboutStartup, "startup", Rocket, Terracotta 5;
    Storage: PanelStorage, AboutStorage, "storage", Drive, Conifer 5;
    Printers: PanelPrinters, AboutPrinters, "printers", Printer, Graphite 4;
    About: PanelAbout, AboutSystem, "about", Info, Amber 5;
}

#[derive(Debug, Clone, Copy)]
pub struct Group {
    pub title: Key,
    pub panels: &'static [Panel],
}

impl Panel {
    pub const GROUPS: &'static [Group] = &[
        Group { title: Key::GroupConnections, panels: &[Self::Network, Self::Bluetooth, Self::Vpn, Self::Netbird] },
        Group { title: Key::GroupAlerts, panels: &[Self::Notifications, Self::Sound] },
        Group {
            title: Key::GroupPersonalisation,
            panels: &[
                Self::Appearance,
                Self::Wallpaper,
                Self::Desktop,
                Self::TopBar,
                Self::Windows,
                Self::Multitasking,
            ],
        },
        Group { title: Key::GroupHardware, panels: &[Self::Displays, Self::NightLight, Self::Power, Self::Printers] },
        Group { title: Key::GroupInput, panels: &[Self::Keyboard, Self::Shortcuts, Self::Mouse, Self::Gestures] },
        Group { title: Key::GroupPeople, panels: &[Self::Accessibility, Self::Privacy, Self::Users] },
        Group {
            title: Key::GroupSystem,
            panels: &[
                Self::DateTime,
                Self::Region,
                Self::Apps,
                Self::Terminal,
                Self::Startup,
                Self::Storage,
                Self::About,
            ],
        },
    ];

    #[must_use]
    pub fn group(self) -> &'static Group {
        Self::GROUPS.iter().find(|group| group.panels.contains(&self)).unwrap_or(&Self::GROUPS[Self::GROUPS.len() - 1])
    }

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
            "lock" | "security" => Some(Self::Privacy),
            "autostart" => Some(Self::Startup),
            "disk" => Some(Self::Storage),
            "keybinds" => Some(Self::Shortcuts),
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
