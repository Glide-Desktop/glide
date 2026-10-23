use glide_sdk::theme::TextStyle;
use glide_sdk::widget::text;
use iced::widget::{column, container, row, space};
use iced::{Alignment, Element, Fill, Length, Task};

use super::common::gated;
use super::{Ctx, Pane, Panel};
use crate::icons::Icon;
use crate::services::config::ConfigStore;
use crate::services::keybinds::{self, Action, Category, Keybind, Mod};
use crate::theme;
use crate::widgets::{self, Group, Row};

mod tr {
    crate::i18n::catalogue! {
        HeroCount => "shortcuts", "сполучень";
        HeroDefaults => "Using the Glide defaults. Click a shortcut to change it.", "Використовуються типові сполучення Glide. Натисніть на сполучення, щоб змінити його.";
        HeroCustomised => "You have changed the defaults. Reset brings them all back.", "Ви змінили типові сполучення. «Скинути все» поверне їх.";
        ResetAll => "Reset all", "Скинути все";
        Unreadable => "The keybinds list in glide.toml cannot be edited here:", "Список keybinds у glide.toml не можна редагувати тут:";
        ConflictsFound => "Some shortcuts share a key combination, so only one of each pair works:", "Деякі сполучення збігаються, тож із кожної пари працює лише одне:";
        FilterPlaceholder => "Filter shortcuts", "Пошук сполучень";
        NothingMatches => "No shortcuts match", "Немає відповідних сполучень";
        NothingMatchesHint => "Try another word or clear the filter", "Спробуйте інше слово або очистьте пошук";

        GroupWindows => "Windows", "Вікна";
        GroupWindowsHint => "Arrange and close the focused window", "Розташування й закриття активного вікна";
        GroupWorkspaces => "Workspaces", "Робочі простори";
        GroupWorkspacesHint => "Move between workspaces and open the overview", "Перехід між просторами та огляд";
        GroupSwitching => "Switching", "Перемикання";
        GroupSwitchingHint => "Switchers for apps and their windows", "Перемикачі програм і їхніх вікон";
        GroupSystem => "System", "Система";
        GroupSystemHint => "Launcher, screenshots, lock and the session", "Лаунчер, знімки екрана, блокування й сеанс";
        GroupMedia => "Media & hardware keys", "Медіа й апаратні клавіші";
        GroupMediaHint => "Volume, brightness and playback", "Гучність, яскравість і відтворення";
        GroupCustom => "Custom shortcuts", "Власні сполучення";
        GroupCustomHint => "Run any command, or bind an action that has no key yet", "Запуск будь-якої команди або дії, яка ще не має клавіші";

        CloseWindow => "Close window", "Закрити вікно";
        CloseApp => "Quit the app", "Закрити програму";
        Maximize => "Maximise", "Розгорнути";
        Fullscreen => "Full screen", "На весь екран";
        Minimize => "Minimise", "Згорнути";
        WindowMenu => "Window menu", "Меню вікна";
        ShowDesktop => "Show the desktop", "Показати робочий стіл";
        FocusTopBar => "Move focus to the top bar", "Перейти до верхньої панелі";
        FocusDock => "Move focus to the dock", "Перейти до панелі програм";
        RunPrompt => "Open the Run prompt", "Відкрити рядок «Виконати»";
        ForceQuit => "Force quit the app", "Завершити програму примусово";
        FillPassword => "Fill a saved password", "Вставити збережений пароль";
        ScreenMirroring => "Mirror the screen to a TV", "Транслювати екран на телевізор";
        TileLeft => "Fill the left half", "Зайняти ліву половину";
        TileRight => "Fill the right half", "Зайняти праву половину";
        TileMaximize => "Fill the screen", "Розгорнути на весь екран";
        TileRestore => "Restore or minimise", "Відновити або згорнути";
        MonitorLeft => "Move window to the monitor on the left", "Перемістити вікно на монітор ліворуч";
        MonitorRight => "Move window to the monitor on the right", "Перемістити вікно на монітор праворуч";
        Terminal => "Open a terminal", "Відкрити термінал";
        MoveToNextWorkspace => "Move window to the next workspace", "Перемістити вікно на наступний простір";
        MoveToPrevWorkspace => "Move window to the previous workspace", "Перемістити вікно на попередній простір";
        NotificationCenter => "Show notifications and calendar", "Показати сповіщення й календар";
        QuickSettings => "Show quick settings", "Показати швидкі налаштування";
        DoNotDisturb => "Toggle Do Not Disturb", "Перемкнути «Не турбувати»";
        Caffeine => "Keep the screen on", "Не вимикати екран";
        NextWorkspace => "Next workspace", "Наступний робочий простір";
        PrevWorkspace => "Previous workspace", "Попередній робочий простір";
        Overview => "Overview", "Огляд";
        SwitchToWorkspace => "Switch to workspace", "Перейти на робочий простір";
        MoveToWorkspace => "Move window to workspace", "Перемістити вікно на простір";
        SwitchApps => "Switch apps", "Перемкнути програму";
        SwitchAppsBack => "Switch apps backwards", "Перемкнути програму назад";
        SwitchWindows => "Switch windows of the app", "Перемкнути вікна програми";
        SwitchWindowsBack => "Switch windows of the app backwards", "Перемкнути вікна програми назад";
        Launcher => "Show the launcher", "Показати лаунчер";
        Screenshot => "Take a screenshot", "Зробити знімок екрана";
        ScreenshotScreen => "Screenshot the whole screen", "Знімок усього екрана";
        ScreenshotWindow => "Screenshot the active window", "Знімок активного вікна";
        PowerDialog => "Power off, restart or log out", "Вимкнення, перезапуск або вихід";
        Logout => "Log out", "Вийти із сеансу";
        Quit => "Quit Glide immediately", "Негайно закрити Glide";
        Console => "Switch to console", "Перейти до консолі";
        LockScreen => "Lock the screen", "Заблокувати екран";
        VolumeUp => "Volume up", "Гучніше";
        VolumeDown => "Volume down", "Тихіше";
        VolumeMute => "Mute sound", "Вимкнути звук";
        MicMute => "Mute the microphone", "Вимкнути мікрофон";
        BrightnessUp => "Brighter screen", "Яскравіше";
        BrightnessDown => "Dimmer screen", "Темніше";
        PlayPause => "Play or pause", "Відтворення або пауза";
        NextTrack => "Next track", "Наступний трек";
        PrevTrack => "Previous track", "Попередній трек";
        Play => "Play", "Відтворити";
        Pause => "Pause", "Пауза";
        Stop => "Stop", "Зупинити";
        SeekForward => "Skip forward 10 s", "Уперед на 10 с";
        SeekBackward => "Skip back 10 s", "Назад на 10 с";
        RunCommand => "Run a command", "Виконати команду";
        CustomCommand => "Custom command", "Власна команда";
        Changed => "Changed", "Змінено";
        ConflictsWith => "Same keys as", "Ті самі клавіші, що й";
        NoKey => "No key", "Без клавіші";

        AddCommand => "Add a command shortcut", "Додати сполучення для команди";
        AddCommandHint => "For example, open a terminal", "Наприклад, відкрити термінал";
        BindAction => "Add a shortcut for an action", "Додати сполучення для дії";
        ChooseAction => "Choose an action…", "Оберіть дію…";
        Modifiers => "Modifiers", "Модифікатори";
        KeyPlaceholder => "Key: q, Return, F5…", "Клавіша: q, Return, F5…";
        CommandPlaceholder => "Command, e.g. alacritty", "Команда, наприклад alacritty";
        Save => "Save", "Зберегти";
        Cancel => "Cancel", "Скасувати";
        Remove => "Remove", "Видалити";
        RestoreDefault => "Use default", "Типове";
        NeedKey => "Type a key name", "Уведіть назву клавіші";
        UnknownKey => "Unknown key. Use a letter, a digit, F1–F35 or an xkb name such as Return", "Невідома клавіша. Використайте літеру, цифру, F1–F35 або назву xkb, наприклад Return";
        NeedModifier => "Add a modifier, otherwise this key could no longer be typed", "Додайте модифікатор, інакше цю клавішу не можна буде друкувати";
        NeedCommand => "Enter the command to run", "Уведіть команду для запуску";
        AlreadyUsed => "Already used by", "Уже використовується для";
        Record => "Record", "Записати";
        Recording => "Press the shortcut… (Esc stops)", "Натисніть комбінацію… (Esc — зупинити)";
        RecordHint => "Shortcuts Glide already uses never reach Settings; type those instead", "Комбінації, які вже використовує Glide, не доходять до Settings — введіть їх вручну";
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Capture {
    Ignore,
    Stop,
    Combo(Vec<Mod>, String),
}

fn named_key(named: iced::keyboard::key::Named) -> Option<String> {
    use iced::keyboard::key::Named as N;
    let name = match named {
        N::Enter => "Return",
        N::Tab => "Tab",
        N::Space => "space",
        N::Escape => "Escape",
        N::Backspace => "BackSpace",
        N::Delete => "Delete",
        N::Insert => "Insert",
        N::Home => "Home",
        N::End => "End",
        N::PageUp => "Prior",
        N::PageDown => "Next",
        N::ArrowUp => "Up",
        N::ArrowDown => "Down",
        N::ArrowLeft => "Left",
        N::ArrowRight => "Right",
        N::PrintScreen => "Print",
        N::AudioVolumeUp => "XF86AudioRaiseVolume",
        N::AudioVolumeDown => "XF86AudioLowerVolume",
        N::AudioVolumeMute => "XF86AudioMute",
        N::MicrophoneToggle => "XF86AudioMicMute",
        N::MediaPlayPause | N::MediaPlay => "XF86AudioPlay",
        N::MediaPause => "XF86AudioPause",
        N::MediaStop => "XF86AudioStop",
        N::MediaFastForward => "XF86AudioForward",
        N::MediaRewind => "XF86AudioRewind",
        N::MediaTrackNext => "XF86AudioNext",
        N::MediaTrackPrevious => "XF86AudioPrev",
        N::BrightnessUp => "XF86MonBrightnessUp",
        N::BrightnessDown => "XF86MonBrightnessDown",
        N::PowerOff => "XF86PowerOff",
        other => {
            let debug = format!("{other:?}");
            return debug
                .strip_prefix('F')
                .and_then(|number| number.parse::<u8>().ok())
                .filter(|number| (1..=35).contains(number))
                .map(|number| format!("F{number}"));
        }
    };
    Some(name.to_string())
}

fn physical_key_name(physical: iced::keyboard::key::Physical) -> Option<String> {
    use iced::keyboard::key::{Code, Physical};
    let Physical::Code(code) = physical else {
        return None;
    };
    let name = match code {
        Code::KeyA => "a",
        Code::KeyB => "b",
        Code::KeyC => "c",
        Code::KeyD => "d",
        Code::KeyE => "e",
        Code::KeyF => "f",
        Code::KeyG => "g",
        Code::KeyH => "h",
        Code::KeyI => "i",
        Code::KeyJ => "j",
        Code::KeyK => "k",
        Code::KeyL => "l",
        Code::KeyM => "m",
        Code::KeyN => "n",
        Code::KeyO => "o",
        Code::KeyP => "p",
        Code::KeyQ => "q",
        Code::KeyR => "r",
        Code::KeyS => "s",
        Code::KeyT => "t",
        Code::KeyU => "u",
        Code::KeyV => "v",
        Code::KeyW => "w",
        Code::KeyX => "x",
        Code::KeyY => "y",
        Code::KeyZ => "z",
        Code::Digit0 => "0",
        Code::Digit1 => "1",
        Code::Digit2 => "2",
        Code::Digit3 => "3",
        Code::Digit4 => "4",
        Code::Digit5 => "5",
        Code::Digit6 => "6",
        Code::Digit7 => "7",
        Code::Digit8 => "8",
        Code::Digit9 => "9",
        Code::Backquote => "grave",
        Code::Minus => "minus",
        Code::Equal => "equal",
        Code::BracketLeft => "bracketleft",
        Code::BracketRight => "bracketright",
        Code::Backslash => "backslash",
        Code::Semicolon => "semicolon",
        Code::Quote => "apostrophe",
        Code::Comma => "comma",
        Code::Period => "period",
        Code::Slash => "slash",
        _ => return None,
    };
    Some(name.to_string())
}

fn active_mods(modifiers: iced::keyboard::Modifiers) -> Vec<Mod> {
    let mut mods = Vec::new();
    for (held, m) in [
        (modifiers.logo(), Mod::Super),
        (modifiers.control(), Mod::Ctrl),
        (modifiers.alt(), Mod::Alt),
        (modifiers.shift(), Mod::Shift),
    ] {
        if held {
            mods.push(m);
        }
    }
    mods
}

#[must_use]
pub fn capture(key: &iced::keyboard::Key, modifiers: iced::keyboard::Modifiers) -> Capture {
    use iced::keyboard::Key;
    let mods = active_mods(modifiers);
    let name = match key {
        Key::Named(iced::keyboard::key::Named::Escape) if mods.is_empty() => return Capture::Stop,
        Key::Named(named) => named_key(*named),
        Key::Character(text) => keybinds::normalize_key(text),
        Key::Unidentified => None,
    };
    name.map_or(Capture::Ignore, |name| Capture::Combo(mods, name))
}

#[must_use]
pub fn capture_event(
    key: &iced::keyboard::Key, physical_key: iced::keyboard::key::Physical, modifiers: iced::keyboard::Modifiers,
) -> Capture {
    match capture(key, modifiers) {
        Capture::Ignore => {
            physical_key_name(physical_key).map_or(Capture::Ignore, |name| Capture::Combo(active_mods(modifiers), name))
        }
        other => other,
    }
}

#[must_use]
pub fn recorded(event: iced::keyboard::Event) -> Option<Msg> {
    let iced::keyboard::Event::KeyPressed { key, physical_key, modifiers, .. } = event else {
        return None;
    };
    match capture_event(&key, physical_key, modifiers) {
        Capture::Ignore => None,
        Capture::Stop => Some(Msg::StopRecording),
        Capture::Combo(mods, key) => Some(Msg::Captured(mods, key)),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub index: Option<usize>,
    pub mods: Vec<Mod>,
    pub key: String,
    pub action: Action,
}

impl Draft {
    #[must_use]
    pub fn bind(&self) -> Option<Keybind> {
        let key = keybinds::normalize_key(&self.key)?;
        let mut mods = self.mods.clone();
        mods.sort_unstable();
        mods.dedup();
        Some(Keybind {
            mods,
            key,
            action: match &self.action {
                Action::Exec(command) => Action::Exec(command.trim().to_string()),
                other => other.clone(),
            },
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftError {
    NeedKey,
    UnknownKey,
    NeedModifier,
    NeedCommand,
}

impl DraftError {
    const fn text(self) -> tr::Key {
        match self {
            Self::NeedKey => tr::Key::NeedKey,
            Self::UnknownKey => tr::Key::UnknownKey,
            Self::NeedModifier => tr::Key::NeedModifier,
            Self::NeedCommand => tr::Key::NeedCommand,
        }
    }
}

const TYPING_KEYS: &[&str] = &[
    "space",
    "Return",
    "Tab",
    "BackSpace",
    "Delete",
    "Escape",
    "grave",
    "minus",
    "equal",
    "bracketleft",
    "bracketright",
    "backslash",
    "semicolon",
    "apostrophe",
    "comma",
    "period",
    "slash",
    "Up",
    "Down",
    "Left",
    "Right",
    "Home",
    "End",
];

pub fn validate(draft: &Draft) -> Result<Keybind, DraftError> {
    if draft.key.trim().is_empty() && draft.key != " " {
        return Err(DraftError::NeedKey);
    }
    let bind = draft.bind().ok_or(DraftError::UnknownKey)?;
    if let Action::Exec(command) = &bind.action
        && command.is_empty()
    {
        return Err(DraftError::NeedCommand);
    }
    let typed = bind.key.chars().count() == 1 || TYPING_KEYS.contains(&bind.key.as_str());
    let has_modifier = bind.mods.iter().any(|m| *m != Mod::Shift);
    if typed && !has_modifier {
        return Err(DraftError::NeedModifier);
    }
    Ok(bind)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionChoice {
    pub name: &'static str,
    pub label: &'static str,
}

impl std::fmt::Display for ActionChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label)
    }
}

#[derive(Debug, Clone)]
pub enum Msg {
    Filter(String),
    Edit(usize),
    AddCommand,
    AddAction(ActionChoice),
    ToggleMod(Mod),
    Key(String),
    Command(String),
    Save,
    Cancel,
    Remove,
    RestoreDefault,
    ResetAll,
    Record,
    StopRecording,
    Captured(Vec<Mod>, String),
}

#[derive(Debug, Default)]
pub struct Page {
    filter: String,
    draft: Option<Draft>,
    recording: bool,
}

impl Page {
    #[must_use]
    pub const fn is_recording(&self) -> bool {
        self.recording
    }
}

impl Pane for Page {
    type Msg = Msg;

    const KEYWORDS: &'static [&'static str] = &[
        "shortcut",
        "keybind",
        "hotkey",
        "binding",
        "combination",
        "сполучення",
        "гарячі клавіші",
        "комбінац",
        "скорочення",
    ];

    fn open(&mut self, _c: Ctx, _config: &ConfigStore) -> Task<Msg> {
        self.recording = false;
        Task::none()
    }

    fn leave(&mut self) {
        self.recording = false;
    }

    fn update(&mut self, msg: Msg, _c: Ctx, config: &mut ConfigStore) -> Task<Msg> {
        match msg {
            Msg::Record => self.recording = self.draft.is_some(),
            Msg::StopRecording => self.recording = false,
            Msg::Captured(mods, key) => {
                if let Some(draft) = self.draft.as_mut().filter(|_| self.recording) {
                    draft.mods = mods;
                    draft.key = key;
                }
                self.recording = false;
            }
            other => {
                if matches!(
                    other,
                    Msg::Save
                        | Msg::Cancel
                        | Msg::Remove
                        | Msg::Edit(_)
                        | Msg::AddCommand
                        | Msg::AddAction(_)
                        | Msg::ResetAll
                ) {
                    self.recording = false;
                }
                self.apply(other, config);
            }
        }
        Task::none()
    }

    fn view<'a>(&'a self, c: Ctx, config: &'a ConfigStore) -> Element<'a, Msg> {
        let t = c.theme;
        let mut page = column![].spacing(theme::SECTION_GAP);
        let loaded = match keybinds::load(config) {
            Ok(loaded) => loaded,
            Err(err) => {
                return page
                    .push(widgets::banner(
                        t,
                        crate::theme::Tone::Danger,
                        Icon::Warning,
                        format!("{} {err}", c.t(tr::Key::Unreadable)),
                        None,
                    ))
                    .into();
            }
        };
        let binds = loaded.binds;
        let reset =
            widgets::secondary(t, c.t(tr::Key::ResetAll), (loaded.customised && c.writable).then_some(Msg::ResetAll));
        page = page.push(widgets::section_hero(
            t,
            Icon::Command,
            Panel::Shortcuts.color(),
            format!("{} {}", binds.len(), c.t(tr::Key::HeroCount)),
            c.t(if loaded.customised { tr::Key::HeroCustomised } else { tr::Key::HeroDefaults }),
            Some(reset),
        ));

        let conflicts = keybinds::conflicts(&binds);
        if !conflicts.is_empty() {
            let list = conflicts
                .iter()
                .map(|(a, b)| {
                    format!(
                        "{} ({} / {})",
                        combo_text(&binds[*a]),
                        action_label(c, &binds[*a].action),
                        action_label(c, &binds[*b].action)
                    )
                })
                .collect::<Vec<_>>()
                .join("; ");
            page = page.push(widgets::banner(
                t,
                crate::theme::Tone::Warn,
                Icon::Warning,
                format!("{} {list}", c.t(tr::Key::ConflictsFound)),
                None,
            ));
        }

        page = page.push(
            widgets::field(t, c.t(tr::Key::FilterPlaceholder), &self.filter, Msg::Filter, None, false).width(Fill),
        );

        let mut shown = 0;
        for category in Category::ALL {
            let (title, hint) = category_text(category);
            let mut group = Group::new(t).title(c.t(title)).description(c.t(hint));
            for (index, bind) in binds.iter().enumerate() {
                if bind.action.category() != category || !self.matches(&c, bind) {
                    continue;
                }
                shown += 1;
                group = group.push(self.bind_row(&c, &binds, index));
            }
            if category == Category::Custom {
                group = self.custom_rows(&c, &binds, group);
            }
            if !group.is_empty() {
                page = page.push(group.build());
            }
        }
        if shown == 0 && !self.filter.trim().is_empty() {
            page = page.push(widgets::list_empty(
                t,
                Icon::Search,
                c.t(tr::Key::NothingMatches),
                c.t(tr::Key::NothingMatchesHint),
            ));
        }
        page.into()
    }
}

fn new_draft(action: Action) -> Draft {
    Draft { index: None, mods: vec![Mod::Super], key: String::new(), action }
}

fn named_label(name: &str) -> Option<tr::Key> {
    Some(match name {
        "close-window" => tr::Key::CloseWindow,
        "close-app" => tr::Key::CloseApp,
        "maximize" => tr::Key::Maximize,
        "fullscreen" => tr::Key::Fullscreen,
        "minimize" => tr::Key::Minimize,
        "open-window-menu" => tr::Key::WindowMenu,
        "show-desktop" => tr::Key::ShowDesktop,
        "focus-top-bar" => tr::Key::FocusTopBar,
        "focus-dock" => tr::Key::FocusDock,
        "run-command" => tr::Key::RunPrompt,
        "force-quit" => tr::Key::ForceQuit,
        "fill-password" => tr::Key::FillPassword,
        "screen-mirroring" => tr::Key::ScreenMirroring,
        "tile-left" => tr::Key::TileLeft,
        "tile-right" => tr::Key::TileRight,
        "tile-maximize" => tr::Key::TileMaximize,
        "tile-restore" => tr::Key::TileRestore,
        "move-window-to-monitor-left" => tr::Key::MonitorLeft,
        "move-window-to-monitor-right" => tr::Key::MonitorRight,
        "terminal" => tr::Key::Terminal,
        "move-window-to-next-workspace" => tr::Key::MoveToNextWorkspace,
        "move-window-to-prev-workspace" => tr::Key::MoveToPrevWorkspace,
        "toggle-notification-center" => tr::Key::NotificationCenter,
        "toggle-quick-settings" => tr::Key::QuickSettings,
        "toggle-do-not-disturb" => tr::Key::DoNotDisturb,
        "toggle-caffeine" => tr::Key::Caffeine,
        "next-workspace" => tr::Key::NextWorkspace,
        "prev-workspace" => tr::Key::PrevWorkspace,
        "toggle-overview" => tr::Key::Overview,
        "switch-windows" => tr::Key::SwitchApps,
        "switch-windows-backward" => tr::Key::SwitchAppsBack,
        "switch-app-windows" => tr::Key::SwitchWindows,
        "switch-app-windows-backward" => tr::Key::SwitchWindowsBack,
        "toggle-launcher" => tr::Key::Launcher,
        "screenshot" => tr::Key::Screenshot,
        "screenshot-screen" => tr::Key::ScreenshotScreen,
        "screenshot-window" => tr::Key::ScreenshotWindow,
        "lock" => tr::Key::LockScreen,
        "power-dialog" => tr::Key::PowerDialog,
        "logout" => tr::Key::Logout,
        "quit" => tr::Key::Quit,
        "media-play-pause" => tr::Key::PlayPause,
        "media-play" => tr::Key::Play,
        "media-pause" => tr::Key::Pause,
        "media-stop" => tr::Key::Stop,
        "media-next" => tr::Key::NextTrack,
        "media-previous" => tr::Key::PrevTrack,
        "media-forward" => tr::Key::SeekForward,
        "media-rewind" => tr::Key::SeekBackward,
        _ => return None,
    })
}

fn exec_label(command: &str) -> Option<tr::Key> {
    let command = command.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(match command.as_str() {
        "loginctl lock-session" => tr::Key::LockScreen,
        "wpctl set-mute @DEFAULT_AUDIO_SINK@ toggle" => tr::Key::VolumeMute,
        "wpctl set-mute @DEFAULT_AUDIO_SOURCE@ toggle" => tr::Key::MicMute,
        "brightnessctl set +5%" => tr::Key::BrightnessUp,
        "brightnessctl set 5%-" => tr::Key::BrightnessDown,
        "playerctl play-pause" => tr::Key::PlayPause,
        "playerctl next" => tr::Key::NextTrack,
        "playerctl previous" => tr::Key::PrevTrack,
        text if text.starts_with("wpctl set-volume") && text.ends_with('+') => tr::Key::VolumeUp,
        text if text.starts_with("wpctl set-volume") && text.ends_with('-') => tr::Key::VolumeDown,
        _ => return None,
    })
}

#[must_use]
pub fn action_label(c: Ctx, action: &Action) -> String {
    match action {
        Action::Named(name) => named_label(name).map_or_else(|| name.clone(), |key| c.t(key).to_string()),
        Action::SwitchWorkspace(n) => format!("{} {n}", c.t(tr::Key::SwitchToWorkspace)),
        Action::MoveToWorkspace(n) => format!("{} {n}", c.t(tr::Key::MoveToWorkspace)),
        Action::Vt(n) => format!("{} {n}", c.t(tr::Key::Console)),
        Action::Exec(command) => exec_label(command).map_or_else(
            || {
                if command.trim().is_empty() {
                    c.t(tr::Key::RunCommand).to_string()
                } else {
                    command.trim().to_string()
                }
            },
            |key| c.t(key).to_string(),
        ),
    }
}

#[must_use]
pub fn combo_text(bind: &Keybind) -> String {
    let mut parts: Vec<String> = bind.sorted_mods().iter().map(|m| m.label().to_string()).collect();
    parts.push(keybinds::key_label(&bind.key));
    parts.join("+")
}

#[must_use]
pub fn keycaps<'a, M: 'a>(c: Ctx, mods: &[Mod], key: Option<&str>) -> Element<'a, M> {
    let t = c.theme;
    let mut sorted = mods.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let caps = sorted.into_iter().map(|m| widgets::key_cap(t, &widgets::key_symbol(m.key())));
    row(caps)
        .spacing(4)
        .align_y(Alignment::Center)
        .push(match key {
            Some(key) => widgets::key_cap(t, &widgets::key_symbol(key)),
            None => text(c.t(tr::Key::NoKey), TextStyle::CAP).color(t.fg_3()).into(),
        })
        .into()
}

const fn category_text(category: Category) -> (tr::Key, tr::Key) {
    match category {
        Category::Windows => (tr::Key::GroupWindows, tr::Key::GroupWindowsHint),
        Category::Workspaces => (tr::Key::GroupWorkspaces, tr::Key::GroupWorkspacesHint),
        Category::Switching => (tr::Key::GroupSwitching, tr::Key::GroupSwitchingHint),
        Category::System => (tr::Key::GroupSystem, tr::Key::GroupSystemHint),
        Category::Media => (tr::Key::GroupMedia, tr::Key::GroupMediaHint),
        Category::Custom => (tr::Key::GroupCustom, tr::Key::GroupCustomHint),
    }
}

impl Page {
    fn apply(&mut self, msg: Msg, config: &mut ConfigStore) {
        let Ok(loaded) = keybinds::load(config) else {
            self.draft = None;
            if let Msg::Filter(filter) = msg {
                self.filter = filter;
            }
            return;
        };
        let mut binds = loaded.binds;
        match msg {
            Msg::Filter(filter) => self.filter = filter,
            Msg::Edit(index) => {
                self.draft = binds.get(index).map(|bind| Draft {
                    index: Some(index),
                    mods: bind.sorted_mods(),
                    key: bind.key.clone(),
                    action: bind.action.clone(),
                });
            }
            Msg::AddCommand => self.draft = Some(new_draft(Action::Exec(String::new()))),
            Msg::AddAction(choice) => {
                self.draft = Some(new_draft(Action::Named(choice.name.to_string())));
            }
            Msg::ToggleMod(m) => {
                if let Some(draft) = &mut self.draft {
                    if let Some(position) = draft.mods.iter().position(|held| *held == m) {
                        draft.mods.remove(position);
                    } else {
                        draft.mods.push(m);
                        draft.mods.sort_unstable();
                    }
                }
            }
            Msg::Key(key) => {
                if let Some(draft) = &mut self.draft {
                    draft.key = key;
                }
            }
            Msg::Command(command) => {
                if let Some(draft) = &mut self.draft
                    && let Action::Exec(current) = &mut draft.action
                {
                    *current = command;
                }
            }
            Msg::Save => {
                let Some(draft) = &self.draft else {
                    return;
                };
                let Ok(bind) = validate(draft) else {
                    return;
                };
                match draft.index {
                    Some(index) if index < binds.len() => binds[index] = bind,
                    _ => binds.push(bind),
                }
                keybinds::store(config, &binds);
                self.draft = None;
            }
            Msg::Cancel => self.draft = None,
            Msg::Remove => {
                if let Some(Draft { index: Some(index), .. }) = self.draft
                    && index < binds.len()
                    && binds.len() > 1
                {
                    binds.remove(index);
                    keybinds::store(config, &binds);
                }
                self.draft = None;
            }
            Msg::RestoreDefault => {
                if let Some(draft) = &mut self.draft
                    && let Some(default) = keybinds::default_for(&binds, draft.index, &draft.action)
                {
                    draft.mods = default.sorted_mods();
                    draft.key = default.key.clone();
                }
            }
            Msg::ResetAll => {
                keybinds::reset(config);
                self.draft = None;
            }
            Msg::Record | Msg::StopRecording | Msg::Captured(..) => {}
        }
    }

    fn matches(&self, c: &Ctx, bind: &Keybind) -> bool {
        let query = self.filter.trim().to_lowercase();
        if query.is_empty() {
            return true;
        }
        let command = match &bind.action {
            Action::Exec(command) => command.to_lowercase(),
            _ => String::new(),
        };
        action_label(*c, &bind.action).to_lowercase().contains(&query)
            || combo_text(bind).to_lowercase().contains(&query)
            || command.contains(&query)
    }

    fn custom_rows<'a>(&'a self, c: &Ctx, binds: &[Keybind], mut group: Group<'a, Msg>) -> Group<'a, Msg> {
        let t = c.theme;
        if let Some(draft) = self.draft.as_ref().filter(|draft| draft.index.is_none()) {
            group = group.push(Self::editor(c, binds, draft, self.recording));
        }
        let c = *c;
        if !self.filter.trim().is_empty() {
            return group;
        }
        let mut add = Row::new(t, c.t(tr::Key::AddCommand))
            .subtitle(c.t(tr::Key::AddCommandHint))
            .prefix(widgets::icon(Icon::Plus, 18.0, t.acc_text()));
        if c.writable {
            add = add.on_press(Msg::AddCommand);
        }
        group = group.push(add.build());
        let unbound: Vec<ActionChoice> = keybinds::NAMED_ACTIONS
            .iter()
            .filter(|name| !binds.iter().any(|bind| bind.action == Action::Named((**name).to_string())))
            .filter_map(|name| named_label(name).map(|key| ActionChoice { name, label: c.t(key) }))
            .collect();
        if !unbound.is_empty() {
            group = group.push(
                Row::new(t, c.t(tr::Key::BindAction))
                    .prefix(widgets::icon(Icon::Command, 18.0, t.acc_text()))
                    .suffix(gated(c, widgets::chooser(t, unbound, c.t(tr::Key::ChooseAction), Msg::AddAction, 240.0)))
                    .build(),
            );
        }
        group
    }

    fn bind_row<'a>(&'a self, c: &Ctx, binds: &[Keybind], index: usize) -> Element<'a, Msg> {
        let t = c.theme;
        let bind = &binds[index];
        if let Some(draft) = self.draft.as_ref().filter(|draft| draft.index == Some(index)) {
            return Self::editor(c, binds, draft, self.recording);
        }
        let c = *c;
        let conflict =
            binds.iter().enumerate().find(|(other, candidate)| *other != index && candidate.same_combo(bind));
        let changed =
            keybinds::default_for(binds, Some(index), &bind.action).is_some_and(|default| !default.same_combo(bind));
        let subtitle = if let Some((_, other)) = conflict {
            format!("{} «{}»", c.t(tr::Key::ConflictsWith), action_label(c, &other.action))
        } else if let Action::Exec(command) = &bind.action {
            if exec_label(command).is_some() { String::new() } else { c.t(tr::Key::CustomCommand).to_string() }
        } else {
            String::new()
        };
        let mut suffix = row![].spacing(8).align_y(Alignment::Center);
        if changed {
            suffix = suffix.push(widgets::pill(t, theme::Tone::Accent, c.t(tr::Key::Changed)));
        }
        if conflict.is_some() {
            suffix = suffix.push(widgets::icon(Icon::Warning, 16.0, t.warn()));
        }
        suffix = suffix.push(keycaps(c, &bind.mods, Some(&bind.key)));
        let mut line = Row::new(t, action_label(c, &bind.action)).subtitle(subtitle).suffix(suffix);
        if c.writable {
            line = line.on_press(Msg::Edit(index));
        }
        line.build()
    }

    fn editor<'a>(c: &Ctx, binds: &[Keybind], draft: &'a Draft, recording: bool) -> Element<'a, Msg> {
        let c = *c;
        let t = c.theme;
        let result = validate(draft);
        let chips = row(Mod::ALL
            .into_iter()
            .map(|m| widgets::chip(t, m.label(), draft.mods.contains(&m), Some(Msg::ToggleMod(m)))))
        .spacing(6)
        .align_y(Alignment::Center);
        let key_field = widgets::field(
            t,
            c.t(tr::Key::KeyPlaceholder),
            &draft.key,
            Msg::Key,
            Some(Msg::Save),
            !draft.key.trim().is_empty() && keybinds::normalize_key(&draft.key).is_none(),
        )
        .width(Length::Fixed(170.0));
        let normalized = keybinds::normalize_key(&draft.key);
        let mut body = column![
            row![
                text(action_label(c, &draft.action), TextStyle::HEAD).color(t.fg_1()).width(Fill),
                keycaps(c, &draft.mods, normalized.as_deref()),
            ]
            .spacing(12)
            .align_y(Alignment::Center),
            row![
                text(c.t(tr::Key::Modifiers), TextStyle::CALL).color(t.fg_2()),
                chips,
                space().width(Fill),
                key_field,
                if recording {
                    widgets::tonal(t, c.t(tr::Key::Recording), Some(Msg::StopRecording))
                } else {
                    widgets::secondary(t, c.t(tr::Key::Record), Some(Msg::Record))
                },
            ]
            .spacing(10)
            .align_y(Alignment::Center),
            text(c.t(tr::Key::RecordHint), TextStyle::CAP).color(t.fg_3()),
        ]
        .spacing(10);
        if let Action::Exec(command) = &draft.action {
            body = body.push(
                widgets::field(t, c.t(tr::Key::CommandPlaceholder), command, Msg::Command, Some(Msg::Save), false)
                    .width(Fill),
            );
        }
        body =
            body.push(Self::feedback(&c, binds, draft, &result)).push(Self::editor_buttons(&c, binds, draft, &result));
        container(body).padding([10.0, theme::ROW_PADDING_X]).width(Fill).into()
    }

    fn feedback<'a>(
        c: &Ctx, binds: &[Keybind], draft: &Draft, result: &Result<Keybind, DraftError>,
    ) -> Option<Element<'a, Msg>> {
        let t = c.theme;
        match result {
            Err(error) if !(draft.key.is_empty() && *error == DraftError::NeedKey) => {
                Some(text(c.t(error.text()), TextStyle::CAP).color(t.danger()).into())
            }
            Ok(bind) => binds
                .iter()
                .enumerate()
                .find(|(index, other)| Some(*index) != draft.index && other.same_combo(bind))
                .map(|(_, other)| {
                    text(format!("{} «{}»", c.t(tr::Key::AlreadyUsed), action_label(*c, &other.action)), TextStyle::CAP)
                        .color(t.warn())
                        .into()
                }),
            Err(_) => None,
        }
    }

    fn editor_buttons<'a>(
        c: &Ctx, binds: &[Keybind], draft: &Draft, result: &Result<Keybind, DraftError>,
    ) -> Element<'a, Msg> {
        let t = c.theme;
        let default = keybinds::default_for(binds, draft.index, &draft.action);
        let can_restore =
            default.is_some_and(|default| result.as_ref().ok().is_none_or(|bind| !default.same_combo(bind)));
        let mut buttons = row![
            widgets::primary(t, c.t(tr::Key::Save), result.is_ok().then_some(Msg::Save)),
            widgets::secondary(t, c.t(tr::Key::Cancel), Some(Msg::Cancel)),
        ]
        .spacing(8)
        .align_y(Alignment::Center);
        if can_restore {
            buttons = buttons.push(widgets::tonal(t, c.t(tr::Key::RestoreDefault), Some(Msg::RestoreDefault)));
        }
        buttons = buttons.push(space().width(Fill));
        if draft.index.is_some() && binds.len() > 1 {
            buttons = buttons.push(widgets::danger(t, c.t(tr::Key::Remove), Some(Msg::Remove)));
        }
        buttons.into()
    }
}
