#[path = "gestures.rs"]
pub mod gestures;

pub use gestures::*;

use smithay::{
    backend::input::{
        AbsolutePositionEvent, Axis, AxisSource, ButtonState, Event, GestureBeginEvent, GestureEndEvent,
        GesturePinchUpdateEvent as BackendGesturePinchUpdateEvent,
        GestureSwipeUpdateEvent as BackendGestureSwipeUpdateEvent, InputBackend, InputEvent, KeyState,
        KeyboardKeyEvent, PointerAxisEvent, PointerButtonEvent, PointerMotionEvent, TouchEvent as BackendTouchEvent,
    },
    input::{
        keyboard::{FilterResult, Keysym, ModifiersState, keysyms},
        pointer::{
            AxisFrame, ButtonEvent, CursorImageStatus, GestureHoldBeginEvent, GestureHoldEndEvent,
            GesturePinchBeginEvent, GesturePinchEndEvent, GesturePinchUpdateEvent, GestureSwipeBeginEvent,
            GestureSwipeEndEvent, GestureSwipeUpdateEvent, MotionEvent, RelativeMotionEvent,
        },
        touch::{DownEvent, MotionEvent as TouchMotionEvent, UpEvent},
    },
    utils::{Logical, Point, Rectangle, SERIAL_COUNTER},
    wayland::{
        pointer_constraints::{PointerConstraint, with_pointer_constraint},
        seat::WaylandFocus,
    },
};

use crate::{
    launcher::Launcher,
    overview::{Direction, OverviewUi},
    state::GlideState,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    PickerCancel,
    Quit,
    VtSwitch(i32),
    SwitchWorkspace(usize),
    MoveWindowToWorkspace(usize),
    NextWorkspace,
    PrevWorkspace,
    Maximize,
    Fullscreen,
    Minimize,
    CloseFocused,
    OpenWindowMenu,
    ToggleOverview,
    OverviewCancel,
    OverviewSelect,
    OverviewNav(Direction),
    Exec(String),
    Screenshot,
    ScreenshotScreen,
    ScreenshotWindow,
    Shot(crate::shot::ShotKey),
    ToggleLauncher,
    LauncherPage(bool),
    LauncherNav(i32, i32),
    LauncherActivate,
    LauncherBackspace,
    LauncherDelete,
    LauncherDeleteWord,
    LauncherArrow(i32),
    LauncherCaret(crate::launcher::field::CaretMove),
    LauncherSelectAll,
    LauncherPaste,
    LauncherClear,
    LauncherType(char),
    ReplySend,
    ReplyBackspace,
    ReplyCancel,
    ReplyType(char),
    TopbarClosePopup,
    TopbarNav(i32, i32),
    TopbarActivate,
    QsBack,
    QsField(crate::topbar::FieldKey),
    SwitchWindows(bool),
    SwitchAppWindows(bool),
    SwitcherStep(i32),
    SwitcherCommit,
    SwitcherCancel,
    SwitcherAppWindows,
    SwitcherCloseApp,
    SwitcherCloseWindow,
    SwitcherHide,
    CloseApp,
    Lock,
    Logout,
    PowerDialog,
    PowerStep(i32),
    PowerActivate,
    PowerCancel,
    GreeterType(char),
    GreeterBackspace,
    GreeterSubmit,
    GreeterCancel,
    GreeterCycleUser,
    GreeterToggleLayout,
    LockedPowerKey,
    ShowDesktop,
    Tile(crate::tiling::Side),
    TileMaximize,
    TileRestore,
    MoveToOutput(crate::tiling::Side),
    ForceQuit,
    FillPassword,
    ScreenMirroring,
    RestoreShortcuts,
    FocusDock,
    FocusTopBar,
    RunPrompt,
    DockNav(i32),
    DockOpenMenu,
    DockActivate,
    DockLeave,
    DockMenuNav(i32),
    DockMenuActivate,
    Terminal,
    MoveWindowToNeighbour(bool),
    Shell(crate::topbar::TopbarAction),
    Media(MediaKey),
    Polkit(crate::polkit::Input),
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKey {
    PlayPause,
    Play,
    Pause,
    Stop,
    Next,
    Previous,
    Forward,
    Rewind,
}

impl MediaKey {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "media-play-pause" => Self::PlayPause,
            "media-play" => Self::Play,
            "media-pause" => Self::Pause,
            "media-stop" => Self::Stop,
            "media-next" => Self::Next,
            "media-previous" => Self::Previous,
            "media-forward" => Self::Forward,
            "media-rewind" => Self::Rewind,
            _ => return None,
        })
    }

    #[must_use]
    pub fn from_playerctl(command: &str) -> Option<Self> {
        let words: Vec<&str> = command.split_whitespace().collect();
        Some(match words.as_slice() {
            ["playerctl", "play-pause"] => Self::PlayPause,
            ["playerctl", "play"] => Self::Play,
            ["playerctl", "pause"] => Self::Pause,
            ["playerctl", "stop"] => Self::Stop,
            ["playerctl", "next"] => Self::Next,
            ["playerctl", "previous"] => Self::Previous,
            _ => return None,
        })
    }
}

const MEDIA_KEYS: [Keysym; 16] = [
    Keysym::XF86_AudioRaiseVolume,
    Keysym::XF86_AudioLowerVolume,
    Keysym::XF86_AudioMute,
    Keysym::XF86_AudioMicMute,
    Keysym::XF86_AudioPlay,
    Keysym::XF86_AudioPause,
    Keysym::XF86_AudioStop,
    Keysym::XF86_AudioNext,
    Keysym::XF86_AudioPrev,
    Keysym::XF86_AudioForward,
    Keysym::XF86_AudioRewind,
    Keysym::XF86_MonBrightnessUp,
    Keysym::XF86_MonBrightnessDown,
    Keysym::XF86_KbdBrightnessUp,
    Keysym::XF86_KbdBrightnessDown,
    Keysym::XF86_KbdLightOnOff,
];

#[must_use]
pub fn libinput_leds(state: smithay::input::keyboard::LedState) -> smithay::reexports::input::Led {
    use smithay::reexports::input::Led;
    let mut leds = Led::empty();
    for (on, led) in [(state.num, Led::NUMLOCK), (state.caps, Led::CAPSLOCK), (state.scroll, Led::SCROLLLOCK)] {
        if on == Some(true) {
            leds |= led;
        }
    }
    leds
}

#[must_use]
pub const fn keypad_navigation(sym: Keysym) -> Keysym {
    match sym {
        Keysym::KP_Left => Keysym::Left,
        Keysym::KP_Right => Keysym::Right,
        Keysym::KP_Up => Keysym::Up,
        Keysym::KP_Down => Keysym::Down,
        Keysym::KP_Home => Keysym::Home,
        Keysym::KP_End => Keysym::End,
        Keysym::KP_Prior => Keysym::Prior,
        Keysym::KP_Next => Keysym::Next,
        Keysym::KP_Insert => Keysym::Insert,
        Keysym::KP_Delete => Keysym::Delete,
        other => other,
    }
}

fn locked_media_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
) -> Option<KeyAction> {
    let media = MEDIA_KEYS.iter().any(|key| *key == modified_sym || raw_syms.contains(key));
    process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
        .filter(|action| media && matches!(action, KeyAction::Exec(_) | KeyAction::Media(_)))
}

fn picker_shortcut(raw_syms: &[Keysym], modified_sym: Keysym) -> KeyAction {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    if matches(Keysym::Escape) {
        return KeyAction::PickerCancel;
    }
    vt_switch_shortcut(raw_syms, modified_sym).unwrap_or(KeyAction::None)
}

fn power_dialog_shortcut(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> KeyAction {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    if matches(Keysym::Escape) {
        return KeyAction::PowerCancel;
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) || (plain && matches(Keysym::space)) {
        return KeyAction::PowerActivate;
    }
    if matches(Keysym::ISO_Left_Tab) || (matches(Keysym::Tab) && modifiers.shift) {
        return KeyAction::PowerStep(-1);
    }
    if matches(Keysym::Tab) || matches(Keysym::Right) {
        return KeyAction::PowerStep(1);
    }
    if matches(Keysym::Left) {
        return KeyAction::PowerStep(-1);
    }
    vt_switch_shortcut(raw_syms, modified_sym).unwrap_or(KeyAction::None)
}

#[must_use]
pub fn held_trigger(modifiers: &ModifiersState) -> crate::switcher::Trigger {
    crate::switcher::Trigger { alt: modifiers.alt, logo: modifiers.logo, ctrl: modifiers.ctrl }
}

fn switcher_active_shortcut(
    modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym, mode: crate::switcher::Mode,
) -> KeyAction {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let back = if modifiers.shift { -1 } else { 1 };
    if matches(Keysym::Escape) {
        return KeyAction::SwitcherCancel;
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return KeyAction::SwitcherCommit;
    }
    if matches(Keysym::grave) || matches(Keysym::asciitilde) {
        return match mode {
            crate::switcher::Mode::Apps => KeyAction::SwitcherAppWindows,
            crate::switcher::Mode::Windows => KeyAction::SwitcherStep(back),
        };
    }
    if matches(Keysym::ISO_Left_Tab) || (matches(Keysym::Tab) && modifiers.shift) {
        return KeyAction::SwitcherStep(-1);
    }
    if matches(Keysym::Tab) || matches(Keysym::Right) {
        return KeyAction::SwitcherStep(1);
    }
    if matches(Keysym::Left) {
        return KeyAction::SwitcherStep(-1);
    }
    if matches(Keysym::q) || matches(Keysym::Q) {
        return KeyAction::SwitcherCloseApp;
    }
    if matches(Keysym::w) || matches(Keysym::W) {
        return KeyAction::SwitcherCloseWindow;
    }
    if matches(Keysym::h) || matches(Keysym::H) {
        return KeyAction::SwitcherHide;
    }
    vt_switch_shortcut(raw_syms, modified_sym).unwrap_or(KeyAction::None)
}

fn topbar_popup_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
    qs_mode: crate::topbar::QsKeyMode,
) -> Option<KeyAction> {
    use crate::topbar::QsKeyMode;
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    if qs_mode == QsKeyMode::Typing {
        let ctrl_only = modifiers.ctrl && !modifiers.alt && !modifiers.logo;
        if let Some(key) = crate::topbar::field_key(ctrl_only, plain, modified_sym.key_char(), matches) {
            return Some(KeyAction::QsField(key));
        }
    }
    if qs_mode != QsKeyMode::Normal {
        if matches(Keysym::Escape) || matches(Keysym::Left) {
            return Some(KeyAction::QsBack);
        }
        if matches(Keysym::ISO_Left_Tab) || matches(Keysym::Tab) {
            let step = if modifiers.shift { -1 } else { 1 };
            return Some(KeyAction::TopbarNav(0, step));
        }
    }
    if matches(Keysym::Escape) {
        return Some(KeyAction::TopbarClosePopup);
    }
    if matches(Keysym::Left) {
        return Some(KeyAction::TopbarNav(-1, 0));
    }
    if matches(Keysym::Right) {
        return Some(KeyAction::TopbarNav(1, 0));
    }
    if matches(Keysym::Up) {
        return Some(KeyAction::TopbarNav(0, -1));
    }
    if matches(Keysym::Down) {
        return Some(KeyAction::TopbarNav(0, 1));
    }
    if matches(Keysym::Tab) {
        let step = if modifiers.shift { -1 } else { 1 };
        return Some(KeyAction::TopbarNav(step, 0));
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) || (plain && matches(Keysym::space)) {
        return Some(KeyAction::TopbarActivate);
    }
    process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
}

fn dock_menu_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym, keyboard: bool,
) -> Option<KeyAction> {
    let matches = |sym: Keysym| modified_sym == sym || raw_syms.contains(&sym);
    if matches(Keysym::Escape) {
        return Some(KeyAction::TopbarClosePopup);
    }
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    if keyboard && plain {
        if matches(Keysym::Up) {
            return Some(KeyAction::DockMenuNav(-1));
        }
        if matches(Keysym::Down) || matches(Keysym::Tab) {
            return Some(KeyAction::DockMenuNav(1));
        }
        if matches(Keysym::Return) || matches(Keysym::KP_Enter) || matches(Keysym::space) {
            return Some(KeyAction::DockMenuActivate);
        }
    }
    process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
}

const fn keeps_dock_focus(action: &KeyAction) -> bool {
    matches!(
        action,
        KeyAction::None
            | KeyAction::FocusDock
            | KeyAction::DockNav(_)
            | KeyAction::DockOpenMenu
            | KeyAction::DockActivate
            | KeyAction::DockLeave
            | KeyAction::DockMenuNav(_)
            | KeyAction::DockMenuActivate
            | KeyAction::TopbarClosePopup
    )
}

fn dock_keys_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches = |sym: Keysym| modified_sym == sym || raw_syms.contains(&sym);
    if matches(Keysym::Escape) {
        return Some(KeyAction::DockLeave);
    }
    let plain = !modifiers.ctrl && !modifiers.alt && !modifiers.logo;
    if plain {
        if matches(Keysym::Left) || (matches(Keysym::Tab) && modifiers.shift) {
            return Some(KeyAction::DockNav(-1));
        }
        if matches(Keysym::Right) || matches(Keysym::Tab) {
            return Some(KeyAction::DockNav(1));
        }
        if matches(Keysym::Up) || matches(Keysym::Menu) || (matches(Keysym::F10) && modifiers.shift) {
            return Some(KeyAction::DockOpenMenu);
        }
        if matches(Keysym::Return) || matches(Keysym::KP_Enter) || matches(Keysym::space) {
            return Some(KeyAction::DockActivate);
        }
    }
    process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedKeybind {
    pub modifiers: ModifiersState,
    pub keysym: Keysym,
    pub action: KeyAction,
}

fn process_keyboard_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
) -> Option<KeyAction> {
    let matches_key = |entry: &ResolvedKeybind| entry.keysym == modified_sym || raw_syms.contains(&entry.keysym);

    let binding = keybinds.iter().find(|entry| {
        entry.modifiers.logo == modifiers.logo
            && entry.modifiers.shift == modifiers.shift
            && entry.modifiers.ctrl == modifiers.ctrl
            && entry.modifiers.alt == modifiers.alt
            && matches_key(entry)
    });

    if let Some(entry) = binding {
        return Some(entry.action.clone());
    }

    vt_switch_shortcut(raw_syms, modified_sym)
}

fn vt_switch_shortcut(raw_syms: &[Keysym], modified_sym: Keysym) -> Option<KeyAction> {
    raw_syms
        .iter()
        .chain(std::iter::once(&modified_sym))
        .map(|sym| sym.raw())
        .find(|raw| (keysyms::KEY_XF86Switch_VT_1..=keysyms::KEY_XF86Switch_VT_12).contains(raw))
        .map(|raw| KeyAction::VtSwitch((raw - keysyms::KEY_XF86Switch_VT_1 + 1).cast_signed()))
}

fn is_global_shortcut(action: &KeyAction) -> bool {
    matches!(
        action,
        KeyAction::Screenshot
            | KeyAction::ScreenshotScreen
            | KeyAction::ScreenshotWindow
            | KeyAction::Lock
            | KeyAction::PowerDialog
    )
}

fn is_power_key(raw_syms: &[Keysym], modified_sym: Keysym) -> bool {
    modified_sym == Keysym::XF86_PowerOff || raw_syms.contains(&Keysym::XF86_PowerOff)
}

fn survives_inhibit(action: &KeyAction, power_key: bool) -> bool {
    power_key || matches!(action, KeyAction::VtSwitch(_))
}

fn restores_shortcuts(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> bool {
    modifiers.logo
        && !modifiers.ctrl
        && !modifiers.alt
        && (modified_sym == Keysym::Escape || raw_syms.contains(&Keysym::Escape))
}

fn launcher_active_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
) -> KeyAction {
    launcher_shortcut(modifiers, raw_syms, modified_sym)
        .or_else(|| vt_switch_shortcut(raw_syms, modified_sym))
        .or_else(|| {
            process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym)
                .filter(|action| is_global_shortcut(action) || matches!(action, KeyAction::ToggleLauncher))
        })
        .or_else(|| typed_char(modifiers, modified_sym).map(KeyAction::LauncherType))
        .unwrap_or(KeyAction::None)
}

fn reply_active_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
) -> KeyAction {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);

    if matches(Keysym::Escape) {
        return KeyAction::ReplyCancel;
    }
    if matches(Keysym::BackSpace) {
        return KeyAction::ReplyBackspace;
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return KeyAction::ReplySend;
    }

    vt_switch_shortcut(raw_syms, modified_sym)
        .or_else(|| process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym).filter(is_global_shortcut))
        .or_else(|| typed_char(modifiers, modified_sym).map(KeyAction::ReplyType))
        .unwrap_or(KeyAction::None)
}

fn polkit_shortcut(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> KeyAction {
    use crate::polkit::Input;
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let input = if matches(Keysym::Escape) {
        Some(Input::Escape)
    } else if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        Some(Input::Submit)
    } else if matches(Keysym::BackSpace) {
        Some(if modifiers.ctrl { Input::Clear } else { Input::Backspace })
    } else if modifiers.ctrl && !modifiers.alt && (matches(Keysym::u) || matches(Keysym::U)) {
        Some(Input::Clear)
    } else {
        typed_char(modifiers, modified_sym).map(Input::Type)
    };
    vt_switch_shortcut(raw_syms, modified_sym).or_else(|| input.map(KeyAction::Polkit)).unwrap_or(KeyAction::None)
}

fn typed_char(modifiers: &ModifiersState, modified_sym: Keysym) -> Option<char> {
    if modifiers.ctrl || modifiers.alt || modifiers.logo {
        return None;
    }
    modified_sym.key_char().filter(|ch| !ch.is_control())
}

fn wake_typed_char(modifiers: &ModifiersState, modified_sym: Keysym) -> Option<KeyAction> {
    typed_char(modifiers, modified_sym).filter(|ch| !ch.is_whitespace()).map(KeyAction::GreeterType)
}

fn is_layout_switch_key(modified_sym: Keysym) -> bool {
    matches!(
        modified_sym,
        Keysym::ISO_Next_Group | Keysym::ISO_Prev_Group | Keysym::ISO_First_Group | Keysym::ISO_Last_Group
    )
}

fn is_modifier_key(raw_syms: &[Keysym]) -> bool {
    raw_syms.iter().any(|sym| {
        matches!(
            *sym,
            Keysym::Shift_L
                | Keysym::Shift_R
                | Keysym::Control_L
                | Keysym::Control_R
                | Keysym::Alt_L
                | Keysym::Alt_R
                | Keysym::Meta_L
                | Keysym::Meta_R
                | Keysym::Super_L
                | Keysym::Super_R
                | Keysym::Hyper_L
                | Keysym::Hyper_R
                | Keysym::ISO_Level3_Shift
                | Keysym::ISO_Level5_Shift
        )
    })
}

fn intercepts_layout_switch(raw_syms: &[Keysym], modified_sym: Keysym) -> bool {
    is_layout_switch_key(modified_sym) && !is_modifier_key(raw_syms)
}

fn greeter_active_shortcut(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> KeyAction {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);

    if let Some(action) = vt_switch_shortcut(raw_syms, modified_sym) {
        return action;
    }
    if is_power_key(raw_syms, modified_sym) {
        return KeyAction::LockedPowerKey;
    }
    if is_layout_switch_key(modified_sym)
        || (modifiers.logo && matches(Keysym::space))
        || (modifiers.alt && modifiers.shift)
        || (modifiers.ctrl && matches(Keysym::space))
    {
        return KeyAction::GreeterToggleLayout;
    }
    if matches(Keysym::Escape) {
        return KeyAction::GreeterCancel;
    }
    if matches(Keysym::BackSpace) {
        return KeyAction::GreeterBackspace;
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return KeyAction::GreeterSubmit;
    }
    if matches(Keysym::Tab) {
        return KeyAction::GreeterCycleUser;
    }

    typed_char(modifiers, modified_sym).map_or(KeyAction::None, KeyAction::GreeterType)
}

fn launcher_shortcut(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> Option<KeyAction> {
    use crate::launcher::field::CaretMove;
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let ctrl_only = modifiers.ctrl && !modifiers.alt && !modifiers.logo;

    if matches(Keysym::Escape) {
        return Some(KeyAction::LauncherClear);
    }
    if matches(Keysym::BackSpace) {
        return Some(if ctrl_only { KeyAction::LauncherDeleteWord } else { KeyAction::LauncherBackspace });
    }
    if matches(Keysym::Delete) {
        return Some(KeyAction::LauncherDelete);
    }
    if ctrl_only && (matches(Keysym::a) || matches(Keysym::A)) {
        return Some(KeyAction::LauncherSelectAll);
    }
    if ctrl_only && (matches(Keysym::v) || matches(Keysym::V)) {
        return Some(KeyAction::LauncherPaste);
    }
    if matches(Keysym::Return) || matches(Keysym::KP_Enter) {
        return Some(KeyAction::LauncherActivate);
    }
    if matches(Keysym::Page_Up) {
        return Some(KeyAction::LauncherPage(false));
    }
    if matches(Keysym::Page_Down) {
        return Some(KeyAction::LauncherPage(true));
    }
    if matches(Keysym::Home) {
        return Some(KeyAction::LauncherCaret(CaretMove::Home));
    }
    if matches(Keysym::End) {
        return Some(KeyAction::LauncherCaret(CaretMove::End));
    }
    if matches(Keysym::Left) {
        return Some(if ctrl_only {
            KeyAction::LauncherCaret(CaretMove::WordLeft)
        } else {
            KeyAction::LauncherArrow(-1)
        });
    }
    if matches(Keysym::Right) {
        return Some(if ctrl_only {
            KeyAction::LauncherCaret(CaretMove::WordRight)
        } else {
            KeyAction::LauncherArrow(1)
        });
    }
    if matches(Keysym::Up) {
        return Some(KeyAction::LauncherNav(0, -1));
    }
    if matches(Keysym::Down) {
        return Some(KeyAction::LauncherNav(0, 1));
    }
    if matches(Keysym::Tab) {
        let step = if modifiers.shift { -1 } else { 1 };
        return Some(KeyAction::LauncherNav(step, 0));
    }
    None
}

fn overview_active_shortcut(
    keybinds: &[ResolvedKeybind], modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym,
) -> KeyAction {
    overview_shortcut(modifiers, raw_syms, modified_sym)
        .or_else(|| vt_switch_shortcut(raw_syms, modified_sym))
        .or_else(|| {
            process_keyboard_shortcut(keybinds, modifiers, raw_syms, modified_sym).filter(|action| {
                is_global_shortcut(action) || matches!(action, KeyAction::ToggleOverview | KeyAction::ToggleLauncher)
            })
        })
        .unwrap_or(KeyAction::None)
}

fn overview_shortcut(modifiers: &ModifiersState, raw_syms: &[Keysym], modified_sym: Keysym) -> Option<KeyAction> {
    let matches = |want: Keysym| modified_sym == want || raw_syms.contains(&want);
    let plain = |want: Keysym| matches(want) && !modifiers.logo && !modifiers.ctrl && !modifiers.alt;

    if matches(Keysym::Escape) {
        Some(KeyAction::OverviewCancel)
    } else if matches(Keysym::Return) || matches(Keysym::KP_Enter) || plain(Keysym::space) {
        Some(KeyAction::OverviewSelect)
    } else if matches(Keysym::Left) || plain(Keysym::h) {
        Some(KeyAction::OverviewNav(Direction::Left))
    } else if matches(Keysym::Right) || plain(Keysym::l) {
        Some(KeyAction::OverviewNav(Direction::Right))
    } else if matches(Keysym::Up) || plain(Keysym::k) {
        Some(KeyAction::OverviewNav(Direction::Up))
    } else if matches(Keysym::Down) || plain(Keysym::j) {
        Some(KeyAction::OverviewNav(Direction::Down))
    } else {
        None
    }
}

#[must_use]
pub fn touch_output_index(names: &[String]) -> Option<usize> {
    if names.is_empty() {
        return None;
    }
    let internal = names.iter().position(|name| crate::lid::is_internal(name));
    Some(internal.unwrap_or(0))
}

fn scroll_delta<I: InputBackend>(event: &I::PointerAxisEvent) -> crate::system_info::ScrollDelta {
    use crate::system_info::ScrollDelta;
    match (event.source(), event.amount_v120(Axis::Vertical)) {
        (AxisSource::Wheel | AxisSource::WheelTilt, Some(v120)) => ScrollDelta::Wheel(v120),
        (AxisSource::Wheel | AxisSource::WheelTilt, None) => {
            ScrollDelta::Wheel(event.amount(Axis::Vertical).unwrap_or(0.0) * 120.0 / 15.0)
        }
        (AxisSource::Finger, _) if event.amount(Axis::Vertical) == Some(0.0) => ScrollDelta::Stop,
        _ => ScrollDelta::Continuous(event.amount(Axis::Vertical).unwrap_or(0.0)),
    }
}

fn axis_amount<I: InputBackend>(event: &I::PointerAxisEvent, axis: Axis) -> f64 {
    event.amount(axis).unwrap_or_else(|| event.amount_v120(axis).unwrap_or(0.0) * 15.0 / 120.0)
}

fn swipe_delta<I: InputBackend>(event: &I::GestureSwipeUpdateEvent, config: &GestureConfig) -> (f64, f64)
where
    I::GestureSwipeUpdateEvent: 'static,
{
    use smithay::reexports::input::event::gesture::{
        GestureEventCoordinates, GestureSwipeUpdateEvent as LibinputSwipe,
    };
    let (dx, dy) = (event as &dyn std::any::Any).downcast_ref::<LibinputSwipe>().map_or_else(
        || {
            let delta = event.delta();
            (delta.x, delta.y)
        },
        |raw| (raw.dx_unaccelerated(), raw.dy_unaccelerated()),
    );
    config.delta(dx, dy)
}

fn clamp_to_outputs(pos: Point<f64, Logical>, outputs: &[Rectangle<i32, Logical>]) -> Point<f64, Logical> {
    if outputs.is_empty() {
        return pos;
    }

    let (pos_x, pos_y) = pos.into();

    let mut best: Option<(f64, Point<f64, Logical>)> = None;
    for rect in outputs {
        let left = f64::from(rect.loc.x);
        let top = f64::from(rect.loc.y);
        let right = f64::from(rect.loc.x + rect.size.w - 1).max(left);
        let bottom = f64::from(rect.loc.y + rect.size.h - 1).max(top);

        let clamped: Point<f64, Logical> = (pos_x.clamp(left, right), pos_y.clamp(top, bottom)).into();
        let (dx, dy) = (clamped.x - pos_x, clamped.y - pos_y);
        let distance = dx * dx + dy * dy;

        if distance == 0.0 {
            return pos;
        }
        if best.is_none_or(|(best_distance, _)| distance < best_distance) {
            best = Some((distance, clamped));
        }
    }

    best.map_or(pos, |(_, clamped)| clamped)
}

impl GlideState {
    pub fn process_input_event<I: InputBackend>(&mut self, event: InputEvent<I>)
    where
        I::GestureSwipeUpdateEvent: 'static,
    {
        use crate::idle::Contact;
        if let InputEvent::SwitchToggle { event } = &event {
            self.handle_switch::<I>(event);
            return;
        }
        self.protocols.idle_notifier_state.notify_activity(&self.seat);
        self.idle.activity(std::time::Instant::now());
        if crate::handlers::xdg_activation::user_pressed(&event) {
            self.session.last_user_input = Some(std::time::Instant::now());
        }
        let waking = self.outputs_blanked;
        if waking {
            self.set_outputs_blanked(false);
        }
        let swallowed = match &event {
            InputEvent::PointerButton { event, .. } => {
                let button = Contact::Button(event.button_code());
                match event.state() {
                    ButtonState::Pressed => self.wake_guard.press(waking, button),
                    ButtonState::Released => self.wake_guard.release(&button),
                }
            }
            InputEvent::TouchDown { event } => self.wake_guard.press(waking, Contact::Touch(event.slot())),
            InputEvent::TouchMotion { event } => self.wake_guard.follows(&Contact::Touch(event.slot())),
            InputEvent::TouchUp { event } => self.wake_guard.release(&Contact::Touch(event.slot())),
            InputEvent::TouchCancel { .. } => {
                self.wake_guard.cancel_touches();
                false
            }
            _ => false,
        };
        if swallowed {
            return;
        }
        match event {
            InputEvent::Keyboard { event, .. } => self.handle_keyboard_event::<I>(&event, waking),
            InputEvent::PointerMotion { event, .. } => self.handle_pointer_motion::<I>(&event),
            InputEvent::PointerMotionAbsolute { event, .. } => {
                self.handle_pointer_motion_absolute::<I>(&event);
            }
            InputEvent::PointerButton { event, .. } => self.handle_pointer_button::<I>(&event),
            InputEvent::PointerAxis { event, .. } => self.handle_pointer_axis::<I>(&event),
            InputEvent::TouchDown { .. } if self.shot_active() => {}
            InputEvent::TouchDown { event } => self.handle_touch_down::<I>(&event),
            InputEvent::TouchUp { event } => self.handle_touch_up::<I>(&event),
            InputEvent::TouchMotion { event } => self.handle_touch_motion::<I>(&event),
            InputEvent::TouchFrame { .. } => self.handle_touch_frame(),
            InputEvent::TouchCancel { .. } => self.handle_touch_cancel(),
            InputEvent::GestureSwipeBegin { event } => {
                self.handle_gesture_swipe_begin::<I>(&event);
            }
            InputEvent::GestureSwipeUpdate { event } => {
                self.handle_gesture_swipe_update::<I>(&event);
            }
            InputEvent::GestureSwipeEnd { event } => {
                self.handle_gesture_swipe_end::<I>(&event);
            }
            InputEvent::GesturePinchBegin { event } => {
                self.handle_gesture_pinch_begin::<I>(&event);
            }
            InputEvent::GesturePinchUpdate { event } => {
                self.handle_gesture_pinch_update::<I>(&event);
            }
            InputEvent::GesturePinchEnd { event } => {
                self.handle_gesture_pinch_end::<I>(&event);
            }
            InputEvent::GestureHoldBegin { event } => {
                self.handle_gesture_hold_begin::<I>(&event);
            }
            InputEvent::GestureHoldEnd { event } => {
                self.handle_gesture_hold_end::<I>(&event);
            }
            _ => {}
        }
    }

    fn can_handle_shell_gesture(&self) -> bool {
        if self.screen_locked() || self.shot_active() {
            return false;
        }
        if let Some(pointer) = self.seat.get_pointer()
            && pointer.is_grabbed()
            && !self.drag.active
        {
            return false;
        }
        true
    }

    fn handle_gesture_swipe_begin<I: InputBackend>(&mut self, event: &I::GestureSwipeBeginEvent) {
        if self.screen_locked() {
            self.gestures.target = SwipeTarget::None;
            return;
        }

        let fingers = event.fingers();
        if self.practising() {
            let (practice, sample) = Practice::swipe(fingers, &self.gestures.config);
            self.gestures.practice = Some(practice);
            self.gestures.target = SwipeTarget::Practice;
            self.send_practice(sample);
            return;
        }
        if let Some(role) = self.gestures.config.role(fingers).filter(|_| self.can_handle_shell_gesture()) {
            self.gestures.target = SwipeTarget::Compositor;
            let mut tracker = TouchpadGestureTracker::new(self.shell_base());
            tracker.role = role;
            self.touchpad_gesture = Some(tracker);
            if role == SwipeRole::Shell {
                self.catch_shell_animation();
            }
            return;
        }

        self.gestures.target = SwipeTarget::Client;
        self.release_caught_animation();
        if let Some(pointer) = self.seat.get_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            pointer.gesture_swipe_begin(self, &GestureSwipeBeginEvent { serial, time: event.time(), fingers });
        }
    }

    pub(crate) fn cancel_touchpad_gesture(&mut self) {
        self.gestures.target = SwipeTarget::None;
        self.touchpad_gesture = None;
        self.gestures.practice = None;
        self.discard_tracking_workspace_swipe();
    }

    fn practising(&mut self) -> bool {
        if self.gestures.clients.is_empty() || self.screen_locked() {
            return false;
        }
        self.gestures.clients.prune_closed();
        !self.gestures.clients.is_empty()
    }

    fn send_practice(&mut self, sample: glide_ipc::GestureSample) {
        self.gestures.clients.broadcast(&glide_ipc::Event::Gesture { sample });
    }

    fn drive_compositor_swipe(&mut self, micros: u64, dx: f64, dy: f64) {
        let info = self.touchpad_gesture.as_mut().map(|tracker| {
            let now = tracker.instant_for(micros);
            tracker.update(now, dx, dy);
            (tracker.kind, tracker.role, tracker.shell_position(), tracker.accum_dx)
        });
        let Some((kind, role, position, accum_dx)) = info else {
            return;
        };
        match (role, kind) {
            (_, GestureKind::Undecided) | (SwipeRole::Windows, GestureKind::Overview) => {}
            (SwipeRole::Windows, GestureKind::Workspace) => self.drive_app_switch(accum_dx),
            (SwipeRole::Shell, GestureKind::Workspace)
                if !self.gestures.config.workspace_swipe || self.shell_surface_open() => {}
            (SwipeRole::Shell, GestureKind::Overview) if !self.gestures.config.overview_swipe => {}
            (SwipeRole::Shell, GestureKind::Overview) => self.apply_shell_position(position),
            (SwipeRole::Shell, GestureKind::Workspace) => {
                self.catch_workspace_swipe();
                if self.workspace_swipe.is_none() {
                    let active_ws = self.active_workspace_index().unwrap_or(0);
                    let room_for_new =
                        self.layout.fixed_workspaces().is_none() && !self.layout.last_workspace_is_empty();
                    let total_ws = self.layout.workspace_count() + usize::from(room_for_new);
                    let screen_width = self.output_width_at_pointer();
                    self.workspace_swipe = Some(WorkspaceSwipeState::new(active_ws, total_ws, screen_width));
                }
                if let Some(swipe) = self.workspace_swipe.as_mut() {
                    swipe.update(dx);
                    let (active, width) = (swipe.active_workspace, swipe.screen_width);
                    let eff_x = swipe.effective_x();
                    self.apply_swipe_offset(active, eff_x, width);
                    self.queue_animation_redraw();
                }
            }
        }
    }

    fn arm_edge_swipe(&mut self, slot: smithay::backend::input::TouchSlot, at: Point<f64, Logical>) {
        let config = self.gestures.config;
        if self.gestures.edge.as_ref().is_some_and(|edge| edge.claimed) {
            return;
        }
        let first_finger = self.gestures.touch_points == 0;
        self.gestures.edge = None;
        if !first_finger || !config.enabled || !config.touch_edges {
            return;
        }
        if !self.can_handle_shell_gesture() || self.session_lock.is_some() {
            return;
        }
        let Some(geometry) = self
            .space
            .outputs()
            .filter_map(|output| self.space.output_geometry(output))
            .find(|geometry| geometry.to_f64().contains(at))
        else {
            return;
        };
        let origin = (f64::from(geometry.loc.x), f64::from(geometry.loc.y));
        let size = (f64::from(geometry.size.w), f64::from(geometry.size.h));
        if let Some(edge) = touch_edge(origin, size, (at.x, at.y)) {
            self.gestures.edge =
                Some(EdgeSwipe { slot, edge, origin, size, start: (at.x, at.y), last: (at.x, at.y), claimed: false });
        }
    }

    fn move_edge_swipe(
        &mut self, slot: smithay::backend::input::TouchSlot, micros: u64, at: Point<f64, Logical>,
    ) -> bool {
        let Some(edge) = self.gestures.edge.as_mut().filter(|edge| edge.slot == slot) else {
            return false;
        };
        let previous = if edge.claimed { edge.last } else { edge.start };
        edge.last = (at.x, at.y);
        if !edge.claimed {
            if !edge.pulled_by(TOUCH_PULL / 2.0) {
                return false;
            }
            edge.claimed = true;
            let kind = edge.edge;
            if let Some(touch) = self.seat.get_touch() {
                touch.cancel(self);
            }
            if kind != TouchEdge::Top {
                let mut tracker = TouchpadGestureTracker::new(self.shell_base());
                tracker.kind = match kind {
                    TouchEdge::Left | TouchEdge::Right => GestureKind::Workspace,
                    TouchEdge::Top | TouchEdge::Bottom => GestureKind::Overview,
                };
                self.catch_shell_animation();
                self.touchpad_gesture = Some(tracker);
            }
        }
        let Some(edge) = self.gestures.edge.as_ref() else {
            return true;
        };
        if edge.edge != TouchEdge::Top {
            let (dx, dy) = touch_units(edge.edge, at.x - previous.0, at.y - previous.1, edge.size);
            self.drive_compositor_swipe(micros, dx, dy);
        }
        true
    }

    fn end_edge_swipe(&mut self, cancelled: bool) -> bool {
        let Some(edge) = self.gestures.edge.take() else {
            return false;
        };
        if !edge.claimed {
            return false;
        }
        if edge.edge == TouchEdge::Top {
            if !cancelled && edge.pulled() && self.topbar.open_popup().is_none() {
                self.run_topbar_action(if edge.left_half() {
                    crate::topbar::TopbarAction::ToggleNotificationCenter
                } else {
                    crate::topbar::TopbarAction::ToggleQuickSettings
                });
                self.queue_redraw();
            }
            return true;
        }
        if let Some(tracker) = self.touchpad_gesture.take() {
            self.end_compositor_swipe(&tracker, cancelled);
        }
        true
    }

    fn shell_base(&self) -> f64 {
        if let Some(launcher) = self.launcher.as_ref().filter(|l| !l.is_closing()) {
            return SHELL_OVERVIEW + launcher.openness();
        }
        self.overview.as_ref().map_or(SHELL_DESKTOP, OverviewUi::progress)
    }

    fn shell_surface_open(&self) -> bool {
        let overview = self.overview.as_ref().is_some_and(|ui| !ui.is_closing());
        let launcher = self.launcher.as_ref().is_some_and(|l| !l.is_closing());
        overview || launcher
    }

    fn apply_shell_position(&mut self, position: f64) {
        if position > SHELL_OVERVIEW {
            if self.overview.as_ref().is_some_and(|ui| !ui.is_closing()) {
                self.begin_close_overview(None);
            }
            self.begin_interactive_launcher(position - SHELL_OVERVIEW);
        } else {
            if let Some(launcher) = self.launcher.as_mut() {
                launcher.set_interactive_progress(0.0);
            }
            if self.overview.is_none() {
                self.begin_interactive_overview();
            }
            if let Some(ui) = self.overview.as_mut() {
                ui.set_interactive_progress(position);
            }
        }
        self.queue_animation_redraw();
    }

    fn settle_shell(&mut self, target: f64, velocity: f64) {
        if target >= SHELL_LAUNCHER {
            if self.overview.as_ref().is_some_and(|ui| !ui.is_closing()) {
                self.begin_close_overview(None);
            }
            if self.launcher.is_none() {
                self.begin_interactive_launcher(0.0);
            }
            if let Some(launcher) = self.launcher.as_mut() {
                launcher.release_gesture(true, velocity);
            }
        } else {
            if let Some(launcher) = self.launcher.as_mut() {
                launcher.release_gesture(false, velocity);
            }
            if target >= SHELL_OVERVIEW {
                if self.overview.is_none() {
                    self.begin_interactive_overview();
                }
                if let Some(ui) = self.overview.as_mut() {
                    ui.commit_to_opening(velocity);
                }
            } else if let Some(ui) = self.overview.as_mut() {
                ui.revert_to_closing(velocity);
            }
        }
        self.queue_animation_redraw();
    }

    fn page_shell_surface(&mut self, tracker: &TouchpadGestureTracker, cancelled: bool) {
        let progress = normalized_progress(tracker.accum_dx, SWIPE_DISTANCE).clamp(-1.0, 1.0);
        let Some(target) = decide_workspace_settlement(1, 3, progress, tracker.workspace_velocity(), cancelled).target
        else {
            return;
        };
        let forward = target == glide_layout::WorkspaceTarget::Next;
        if let Some(launcher) = self.launcher.as_mut().filter(|l| !l.is_closing()) {
            if launcher.turn_page(forward) {
                self.queue_redraw();
            }
            return;
        }
        let Some(active) = self.active_workspace_index() else {
            return;
        };
        let neighbour = if forward { active.checked_add(1) } else { active.checked_sub(1) };
        let ids = self.layout.workspace_ids();
        if let Some(&id) = neighbour.and_then(|index| ids.get(index)) {
            self.overview_switch_workspace(id);
        }
    }

    fn drive_app_switch(&mut self, accum_dx: f64) {
        if !self.gestures.app_switching {
            self.open_switcher(false, crate::switcher::Mode::Apps, crate::switcher::Trigger::default());
            self.gestures.app_switching = self.switcher_open();
        }
        if let Some(switcher) = self.switcher.as_mut().filter(|s| !s.is_closing()) {
            let last = switcher.entries().len().saturating_sub(1);
            let index = usize::try_from(app_switch_offset(accum_dx).max(0)).unwrap_or(0).min(last);
            if switcher.select(index) {
                self.queue_redraw();
            }
        }
    }

    fn swipe_window(&mut self, tracker: &TouchpadGestureTracker, cancelled: bool) {
        let Some(id) = self.layout.focused_window() else {
            return;
        };
        let maximized = self.layout.window_state(id).is_some_and(|state| state.maximized);
        match decide_window_swipe(tracker.accum_dy, tracker.overview_velocity(), maximized, cancelled) {
            Some(WindowSwipe::Maximize | WindowSwipe::Restore) => {
                self.morph_geometry(id, |state| {
                    state.layout.toggle_maximized(id);
                });
            }
            Some(WindowSwipe::Minimize) => self.minimize_with_flight(id),
            None => {}
        }
    }

    fn end_compositor_swipe(&mut self, tracker: &TouchpadGestureTracker, cancelled: bool) {
        let config = self.gestures.config;
        match (tracker.role, tracker.kind) {
            (SwipeRole::Shell, GestureKind::Workspace) if config.workspace_swipe => {
                if self.shell_surface_open() {
                    self.page_shell_surface(tracker, cancelled);
                } else {
                    self.settle_workspace_swipe(tracker.workspace_velocity(), cancelled);
                }
            }
            (SwipeRole::Shell, GestureKind::Overview) if config.overview_swipe => {
                self.settle_shell(tracker.shell_target(cancelled), tracker.overview_velocity());
            }
            (SwipeRole::Windows, GestureKind::Overview) => self.swipe_window(tracker, cancelled),
            _ => {}
        }
        if std::mem::take(&mut self.gestures.app_switching) {
            if cancelled {
                self.cancel_switcher();
            } else {
                self.commit_switcher();
            }
        }
        self.release_caught_animation();
    }

    fn catch_shell_animation(&mut self) -> bool {
        let caught_swipe = self.workspace_swipe.as_mut().is_some_and(WorkspaceSwipeState::catch);
        let caught_overview = self.overview.as_mut().is_some_and(OverviewUi::catch);
        let caught_launcher = self.launcher.as_mut().is_some_and(Launcher::catch);
        let caught = caught_swipe || caught_overview || caught_launcher;
        if caught {
            self.queue_animation_redraw();
        }
        caught
    }

    fn release_caught_animation(&mut self) {
        self.settle_workspace_swipe(0.0, false);
        let overview = self.overview.as_ref().filter(|ui| ui.is_interactive()).map(OverviewUi::progress);
        let launcher = self
            .launcher
            .as_ref()
            .filter(|l| l.is_interactive() && l.openness() > 0.0)
            .map(|l| SHELL_OVERVIEW + l.openness());
        if let Some(position) = launcher.or(overview) {
            self.settle_shell(position.round(), 0.0);
        }
    }

    fn catch_workspace_swipe(&mut self) {
        if self.workspace_swipe.as_mut().is_some_and(WorkspaceSwipeState::catch) {
            self.queue_animation_redraw();
        }
    }

    fn settle_workspace_swipe(&mut self, velocity: f64, cancelled: bool) {
        let existing = self.layout.workspace_count();
        let Some(swipe) =
            self.workspace_swipe.as_mut().filter(|swipe| matches!(swipe.phase, WorkspaceSwipePhase::Tracking { .. }))
        else {
            return;
        };
        let active = swipe.active_workspace;
        swipe.settle(velocity, cancelled);
        if let WorkspaceSwipePhase::Settling { target_workspace: Some(target), .. } = &mut swipe.phase {
            *target = swipe_workspace_target(*target, active, existing);
        }
        self.queue_animation_redraw();
    }

    fn discard_tracking_workspace_swipe(&mut self) {
        let tracking = self
            .workspace_swipe
            .as_ref()
            .is_some_and(|swipe| matches!(swipe.phase, WorkspaceSwipePhase::Tracking { .. }));
        if tracking {
            self.workspace_swipe = None;
            self.reconcile_space();
        }
    }

    fn handle_gesture_swipe_update<I: InputBackend>(&mut self, event: &I::GestureSwipeUpdateEvent)
    where
        I::GestureSwipeUpdateEvent: 'static,
    {
        if self.screen_locked() {
            self.cancel_touchpad_gesture();
            return;
        }
        match self.gestures.target {
            SwipeTarget::Practice => {
                let (dx, dy) = swipe_delta::<I>(event, &self.gestures.config);
                let config = self.gestures.config;
                let micros = event.time().micros();
                if let Some(sample) =
                    self.gestures.practice.as_mut().map(|practice| practice.swipe_update(micros, dx, dy, &config))
                {
                    self.send_practice(sample);
                }
            }
            SwipeTarget::Compositor => {
                let (dx, dy) = swipe_delta::<I>(event, &self.gestures.config);
                self.drive_compositor_swipe(event.time().micros(), dx, dy);
            }
            SwipeTarget::Client => {
                if let Some(pointer) = self.seat.get_pointer() {
                    pointer.gesture_swipe_update(
                        self,
                        &GestureSwipeUpdateEvent { time: event.time(), delta: event.delta() },
                    );
                }
            }
            SwipeTarget::None => {}
        }
    }

    fn handle_gesture_swipe_end<I: InputBackend>(&mut self, event: &I::GestureSwipeEndEvent) {
        if self.screen_locked() {
            self.cancel_touchpad_gesture();
            return;
        }
        let target = std::mem::replace(&mut self.gestures.target, SwipeTarget::None);
        match target {
            SwipeTarget::Practice => {
                if let Some(practice) = self.gestures.practice.take() {
                    let sample = practice.end(event.cancelled(), &self.gestures.config);
                    self.send_practice(sample);
                }
            }
            SwipeTarget::Compositor => {
                if let Some(tracker) = self.touchpad_gesture.take() {
                    self.end_compositor_swipe(&tracker, event.cancelled());
                }
            }
            SwipeTarget::Client => {
                if let Some(pointer) = self.seat.get_pointer() {
                    let serial = SERIAL_COUNTER.next_serial();
                    pointer.gesture_swipe_end(
                        self,
                        &GestureSwipeEndEvent { serial, time: event.time(), cancelled: event.cancelled() },
                    );
                }
            }
            SwipeTarget::None => {}
        }
    }

    fn unlocked_pointer(&self) -> Option<smithay::input::pointer::PointerHandle<Self>> {
        (!self.screen_locked()).then(|| self.seat.get_pointer()).flatten()
    }

    fn practice_of(&mut self, kind: glide_ipc::GestureKind) -> Option<&mut Practice> {
        self.gestures.practice.as_mut().filter(|practice| practice.kind() == kind)
    }

    fn end_practice(&mut self, kind: glide_ipc::GestureKind, cancelled: bool) -> bool {
        if self.practice_of(kind).is_none() {
            return false;
        }
        if let Some(practice) = self.gestures.practice.take() {
            let sample = practice.end(cancelled, &self.gestures.config);
            self.send_practice(sample);
        }
        true
    }

    fn handle_gesture_pinch_begin<I: InputBackend>(&mut self, event: &I::GesturePinchBeginEvent) {
        if self.practising() {
            let (practice, sample) = Practice::pinch(event.fingers(), &self.gestures.config);
            self.gestures.practice = Some(practice);
            self.send_practice(sample);
            return;
        }
        self.release_caught_animation();
        if self.gestures.config.claims_pinch(event.fingers())
            && self.can_handle_shell_gesture()
            && !self.layout.overview_active()
        {
            self.gestures.pinching = true;
            self.gestures.pinch_scale = 1.0;
            return;
        }
        if let Some(pointer) = self.unlocked_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            pointer.gesture_pinch_begin(
                self,
                &GesturePinchBeginEvent { serial, time: event.time(), fingers: event.fingers() },
            );
        }
    }

    fn handle_gesture_pinch_update<I: InputBackend>(&mut self, event: &I::GesturePinchUpdateEvent) {
        let delta = event.delta();
        let scale = event.scale();
        if let Some(sample) = self
            .practice_of(glide_ipc::GestureKind::Pinch)
            .map(|practice| practice.pinch_update(delta.x, delta.y, scale))
        {
            self.send_practice(sample);
            return;
        }
        if self.gestures.pinching {
            self.gestures.pinch_scale = scale;
            let progress = pinch_progress(scale).max(0.0);
            if progress > 0.0 || self.launcher.is_some() {
                self.begin_interactive_launcher(progress);
            }
            return;
        }
        if let Some(pointer) = self.unlocked_pointer() {
            pointer.gesture_pinch_update(
                self,
                &GesturePinchUpdateEvent {
                    time: event.time(),
                    delta: event.delta(),
                    scale: event.scale(),
                    rotation: event.rotation(),
                },
            );
        }
    }

    fn handle_gesture_pinch_end<I: InputBackend>(&mut self, event: &I::GesturePinchEndEvent) {
        if self.end_practice(glide_ipc::GestureKind::Pinch, event.cancelled()) {
            return;
        }
        if std::mem::take(&mut self.gestures.pinching) {
            let scale = self.gestures.pinch_scale;
            match decide_pinch(scale, event.cancelled()) {
                Some(PinchAction::Launcher) => self.settle_shell(SHELL_LAUNCHER, 0.0),
                Some(PinchAction::Desktop) => self.toggle_show_desktop(),
                None => {
                    if let Some(launcher) = self.launcher.as_mut().filter(|l| l.is_interactive()) {
                        launcher.release_gesture(false, 0.0);
                        self.queue_animation_redraw();
                    }
                }
            }
            return;
        }
        if let Some(pointer) = self.unlocked_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            pointer.gesture_pinch_end(
                self,
                &GesturePinchEndEvent { serial, time: event.time(), cancelled: event.cancelled() },
            );
        }
    }

    fn handle_gesture_hold_begin<I: InputBackend>(&mut self, event: &I::GestureHoldBeginEvent) {
        if self.practising() {
            let (practice, sample) = Practice::hold(event.fingers());
            self.gestures.practice = Some(practice);
            self.send_practice(sample);
            return;
        }
        if self.gestures.config.claims(event.fingers())
            && self.can_handle_shell_gesture()
            && self.catch_shell_animation()
        {
            self.gestures.holding = true;
            return;
        }
        if let Some(pointer) = self.unlocked_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            pointer.gesture_hold_begin(
                self,
                &GestureHoldBeginEvent { serial, time: event.time(), fingers: event.fingers() },
            );
        }
    }

    fn handle_gesture_hold_end<I: InputBackend>(&mut self, event: &I::GestureHoldEndEvent) {
        if self.end_practice(glide_ipc::GestureKind::Hold, event.cancelled()) {
            return;
        }
        if std::mem::take(&mut self.gestures.holding) {
            if !event.cancelled() {
                self.release_caught_animation();
            }
            return;
        }
        if let Some(pointer) = self.unlocked_pointer() {
            let serial = SERIAL_COUNTER.next_serial();
            pointer.gesture_hold_end(
                self,
                &GestureHoldEndEvent { serial, time: event.time(), cancelled: event.cancelled() },
            );
        }
    }

    fn touch_position<I: InputBackend>(&self, event: &impl AbsolutePositionEvent<I>) -> Option<Point<f64, Logical>> {
        let names: Vec<String> = self.space.outputs().map(smithay::output::Output::name).collect();
        let index = touch_output_index(&names)?;
        let output = self.space.outputs().nth(index)?;
        let geometry = self.space.output_geometry(output)?;
        Some(event.position_transformed(geometry.size) + geometry.loc.to_f64())
    }

    fn handle_touch_down<I: InputBackend>(&mut self, event: &I::TouchDownEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let Some(position) = self.touch_position::<I>(event) else {
            return;
        };
        self.arm_edge_swipe(event.slot(), position);
        self.gestures.touch_points += 1;
        self.queue_redraw();
        let serial = SERIAL_COUNTER.next_serial();
        let on_shell = self.session_lock.is_none() && !touch.is_grabbed() && self.shell_button(position, BTN_LEFT);
        if on_shell {
            self.shell_release(position, BTN_LEFT);
        }
        let under = if on_shell {
            None
        } else {
            self.surface_under(position)
                .and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc.to_f64())))
        };
        touch.down(self, under, &DownEvent { slot: event.slot(), location: position, serial, time: event.time() });
    }

    fn handle_touch_up<I: InputBackend>(&mut self, event: &I::TouchUpEvent) {
        self.gestures.touch_points = self.gestures.touch_points.saturating_sub(1);
        let ours = self.gestures.edge.as_ref().is_some_and(|edge| edge.slot == event.slot());
        if ours && self.end_edge_swipe(false) {
            return;
        }
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let serial = SERIAL_COUNTER.next_serial();
        touch.up(self, &UpEvent { slot: event.slot(), serial, time: event.time() });
    }

    fn handle_touch_motion<I: InputBackend>(&mut self, event: &I::TouchMotionEvent) {
        let Some(touch) = self.seat.get_touch() else {
            return;
        };
        let Some(position) = self.touch_position::<I>(event) else {
            return;
        };
        if self.move_edge_swipe(event.slot(), event.time().micros(), position) {
            return;
        }
        self.queue_redraw();
        if self.drag.touch_at.is_some() {
            self.drag.touch_at = Some(position);
        }
        let under = self
            .surface_under(position)
            .and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc.to_f64())));
        touch.motion(self, under, &TouchMotionEvent { slot: event.slot(), location: position, time: event.time() });
    }

    fn handle_touch_frame(&mut self) {
        if let Some(touch) = self.seat.get_touch() {
            touch.frame(self);
        }
    }

    fn handle_touch_cancel(&mut self) {
        self.gestures.touch_points = 0;
        self.end_edge_swipe(true);
        if let Some(touch) = self.seat.get_touch() {
            touch.cancel(self);
        }
    }

    #[allow(clippy::too_many_lines)]
    fn handle_keyboard_event<I: InputBackend>(&mut self, event: &I::KeyboardKeyEvent, waking: bool) {
        self.restore_seat_keymap();
        let serial = SERIAL_COUNTER.next_serial();
        let time = event.time();
        let key_state = event.state();
        let keycode = event.key_code();
        let mut suppressed_keys = std::mem::take(&mut self.suppressed_keys);
        let keybinds = std::mem::take(&mut self.keybinds);

        let overview_active = self.layout.overview_active();
        let launcher_open = self.launcher.as_ref().is_some_and(|launcher| !launcher.is_closing());
        let replying = self.notifications.reply_target().is_some();
        let topbar_popup = self.topbar.has_open_popup();
        let qs_mode = self.topbar.qs_key_mode();
        let dock_menu = self.dock.menu_rect().is_some();
        let dock_keys = self.dock.keyboard_active();
        let power_open = self.power_dialog_open();
        let picker_open = self.picker_active();
        let shot_open = self.shot_active();
        let shot_typing = self.shot_typing();
        let shot_editing = self.shot_editing();
        let switcher_open = self.switcher_open();
        let switcher_mode = self.switcher.as_ref().map_or(crate::switcher::Mode::Apps, crate::switcher::Switcher::mode);
        let greeter_active = self.greeter.is_some();
        let layout_before = self.active_layout_index();
        let locked = self.session_lock.is_some();
        let polkit_open = !locked && !greeter_active && self.polkit_focused();
        let handoff = self.handoff_blocks_input();
        let handoff_blocking = handoff || waking;
        let wake_types = waking && !handoff && greeter_active;
        if locked {
            self.focus_lock_surface();
        }

        let exclusive_layer = self.exclusive_layer_surface();
        if let Some(layer) = exclusive_layer {
            self.layer_focus = Some(layer);
            self.apply_keyboard_focus();
        }
        let exclusive = self.layer_focus.is_some()
            && self.layer_focus.as_ref().is_some_and(smithay::desktop::LayerSurface::can_receive_keyboard_focus);

        let shortcuts_inhibited = {
            use smithay::wayland::keyboard_shortcuts_inhibit::KeyboardShortcutsInhibitorSeat;
            self.seat.keyboard_shortcuts_inhibited()
        };

        let keyboard = self.seat.get_keyboard().expect("a keyboard was added in GlideState::new");
        let mut osd_kind = None;
        let mut super_key = None;
        let action: Option<KeyAction> =
            keyboard.input(self, keycode, key_state, serial, time, |_, modifiers, handle| {
                let keysym = handle.modified_sym();
                let mut binding_syms = handle.raw_syms();
                if let Some(latin) = handle.raw_latin_sym_or_raw_current_sym()
                    && !binding_syms.contains(&latin)
                {
                    binding_syms.push(latin);
                }

                super_key = Some((
                    crate::super_keys::is_super(&binding_syms, keysym),
                    modifiers.ctrl || modifiers.alt || modifiers.shift,
                ));
                if let KeyState::Pressed = key_state {
                    osd_kind = crate::osd::kind_for_key(&binding_syms, keysym);
                    if handoff_blocking {
                        suppressed_keys.push(keycode);
                        let action = vt_switch_shortcut(&binding_syms, keysym)
                            .or_else(|| wake_types.then(|| wake_typed_char(modifiers, keysym)).flatten());
                        return FilterResult::Intercept(action.unwrap_or(KeyAction::None));
                    }
                    let shell_sym = keypad_navigation(keysym);
                    if greeter_active {
                        let action = locked_media_shortcut(&keybinds, modifiers, &binding_syms, keysym)
                            .unwrap_or_else(|| greeter_active_shortcut(modifiers, &binding_syms, shell_sym));
                        suppressed_keys.push(keycode);
                        return FilterResult::Intercept(action);
                    }
                    if intercepts_layout_switch(&binding_syms, keysym) {
                        suppressed_keys.push(keycode);
                        return FilterResult::Intercept(KeyAction::None);
                    }
                    if locked {
                        let action = vt_switch_shortcut(&binding_syms, keysym)
                            .or_else(|| is_power_key(&binding_syms, keysym).then_some(KeyAction::LockedPowerKey))
                            .or_else(|| locked_media_shortcut(&keybinds, modifiers, &binding_syms, keysym));
                        if let Some(action) = action {
                            suppressed_keys.push(keycode);
                            return FilterResult::Intercept(action);
                        }
                        return FilterResult::Forward;
                    }
                    if polkit_open {
                        suppressed_keys.push(keycode);
                        return FilterResult::Intercept(polkit_shortcut(modifiers, &binding_syms, shell_sym));
                    }
                    let (syms, sym) = (&binding_syms, shell_sym);
                    let action = if shot_open {
                        Some(vt_switch_shortcut(syms, sym).unwrap_or_else(|| {
                            KeyAction::Shot(crate::shot::shortcut(modifiers, syms, sym, shot_typing, shot_editing))
                        }))
                    } else if picker_open {
                        Some(picker_shortcut(syms, sym))
                    } else if power_open {
                        Some(power_dialog_shortcut(modifiers, syms, sym))
                    } else if switcher_open {
                        Some(switcher_active_shortcut(modifiers, syms, sym, switcher_mode))
                    } else if replying && !exclusive {
                        Some(reply_active_shortcut(&keybinds, modifiers, syms, sym))
                    } else if launcher_open && !exclusive {
                        Some(launcher_active_shortcut(&keybinds, modifiers, syms, sym))
                    } else if overview_active {
                        Some(overview_active_shortcut(&keybinds, modifiers, syms, sym))
                    } else if topbar_popup && !exclusive {
                        topbar_popup_shortcut(&keybinds, modifiers, syms, sym, qs_mode)
                    } else if dock_menu && !exclusive {
                        dock_menu_shortcut(&keybinds, modifiers, syms, sym, dock_keys)
                    } else if dock_keys && !exclusive {
                        dock_keys_shortcut(&keybinds, modifiers, syms, sym)
                    } else {
                        process_keyboard_shortcut(&keybinds, modifiers, &binding_syms, keysym)
                    };

                    let action = if shortcuts_inhibited && restores_shortcuts(modifiers, &binding_syms, keysym) {
                        Some(KeyAction::RestoreShortcuts)
                    } else if (exclusive || shortcuts_inhibited) && !power_open {
                        let power_key = is_power_key(&binding_syms, keysym);
                        action.filter(|action| survives_inhibit(action, power_key))
                    } else {
                        action
                    };

                    if action.is_some() {
                        suppressed_keys.push(keycode);
                    }
                    action.map_or(FilterResult::Forward, FilterResult::Intercept)
                } else if suppressed_keys.contains(&keycode) {
                    suppressed_keys.retain(|k| *k != keycode);
                    FilterResult::Intercept(KeyAction::None)
                } else {
                    FilterResult::Forward
                }
            });

        self.suppressed_keys = suppressed_keys;
        self.keybinds = keybinds;

        if let Some((is_super, other_modifiers)) = super_key {
            let tapped = match key_state {
                KeyState::Pressed => {
                    self.session.super_tap.press(is_super, other_modifiers);
                    false
                }
                KeyState::Released => self.session.super_tap.release(is_super),
            };
            if tapped && !self.screen_locked() && !handoff_blocking && !shortcuts_inhibited && !polkit_open {
                self.toggle_launcher();
            }
        }

        let now = std::time::Instant::now();
        match osd_kind {
            Some(crate::osd::Kind::CapsLock) if !waking && !handoff_blocking => {
                let on = keyboard.modifier_state().caps_lock;
                self.session.osd.show(crate::osd::Reading { kind: crate::osd::Kind::CapsLock, level: 0, on }, now);
                self.queue_redraw();
            }
            Some(kind) if matches!(action, Some(KeyAction::Exec(_))) => self.session.osd.expect(kind, now),
            _ => {}
        }

        let mut action = action;
        self.sync_prompt_caps_lock();
        if self.session.polkit.prompt.is_some() {
            self.sync_polkit();
        }
        let switched_by_xkb = self.active_layout_index() != layout_before;
        if switched_by_xkb {
            if action.is_some() {
                keyboard.advertise_modifier_state(self);
            }
            self.layout_switched();
        }
        if greeter_active {
            let caps_lock = keyboard.modifier_state().caps_lock;
            if self.greeter.as_mut().is_some_and(|greeter| greeter.set_caps_lock(caps_lock)) {
                self.queue_redraw();
            }
            if switched_by_xkb {
                if action == Some(KeyAction::GreeterToggleLayout) {
                    action = None;
                }
                self.sync_greeter_layout();
                self.queue_redraw();
            }
        }

        if let Some(action) = action {
            self.run_key_action(action);
        }

        let held = held_trigger(&keyboard.modifier_state());
        if self.switcher_open() && self.switcher.as_ref().is_some_and(|switcher| switcher.trigger().released(held)) {
            self.commit_switcher();
        }
    }

    #[allow(clippy::too_many_lines)]
    pub(crate) fn run_key_action(&mut self, action: KeyAction) {
        if self.dock.keyboard_active() && !keeps_dock_focus(&action) && self.dock.end_keyboard() {
            self.queue_redraw();
        }
        match action {
            KeyAction::Quit => self.loop_signal.stop(),
            KeyAction::VtSwitch(vt) => {
                self.suppressed_keys.clear();
                self.backend_data.change_vt(vt);
            }
            KeyAction::SwitchWorkspace(idx) => {
                tracing::info!(workspace_index = idx, "switching workspace");
                self.switch_workspace_by_key(glide_layout::WorkspaceTarget::Index(idx));
            }
            KeyAction::MoveWindowToWorkspace(idx) => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, workspace_index = idx, "moving window to workspace");
                    self.suppressed_keys.clear();
                    self.layout.move_window_to_workspace(focused, glide_layout::WorkspaceTarget::Index(idx));
                    self.reconcile_space();
                    self.sync_ext_workspaces();
                }
            }
            KeyAction::NextWorkspace => {
                tracing::info!("switching to next workspace");
                self.switch_workspace_by_key(glide_layout::WorkspaceTarget::Next);
            }
            KeyAction::PrevWorkspace => {
                tracing::info!("switching to prev workspace");
                self.switch_workspace_by_key(glide_layout::WorkspaceTarget::Previous);
            }
            KeyAction::Maximize => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "toggling maximize");
                    self.morph_geometry(focused, |state| {
                        state.layout.toggle_maximized(focused);
                    });
                }
            }
            KeyAction::Fullscreen => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "toggling fullscreen");
                    self.morph_geometry(focused, |state| {
                        state.layout.toggle_fullscreen(focused);
                    });
                }
            }
            KeyAction::Minimize => {
                if let Some(focused) = self.layout.focused_window() {
                    tracing::info!(focused = ?focused, "minimizing");
                    self.minimize_with_flight(focused);
                }
            }
            KeyAction::CloseFocused => {
                if let Some(focused) = self.layout.focused_window()
                    && let Some(window) = self.window_map.get(&focused)
                    && let Some(toplevel) = window.toplevel()
                {
                    tracing::info!(focused = ?focused, "closing via keybind");
                    toplevel.send_close();
                }
            }
            KeyAction::OpenWindowMenu => {
                if let Some(focused) = self.layout.focused_window()
                    && let Some(window) = self.window_map.get(&focused)
                    && let Some(mut deco) = window.try_decoration_state_mut()
                {
                    tracing::info!(focused = ?focused, "opening titlebar menu via keybind");
                    deco.header.open_menu_at_default_position();
                }
                self.queue_redraw();
            }
            KeyAction::OverviewCancel => {
                self.begin_close_overview(None);
            }
            KeyAction::OverviewSelect => {
                let select = self.overview.as_ref().and_then(OverviewUi::hovered);
                self.begin_close_overview(select);
            }
            KeyAction::OverviewNav(dir) => self.navigate_overview(dir),
            KeyAction::SwitchWindows(backward) => {
                let trigger = self.current_trigger();
                self.open_switcher(backward, crate::switcher::Mode::Apps, trigger);
            }
            KeyAction::SwitchAppWindows(backward) => {
                let trigger = self.current_trigger();
                self.open_switcher(backward, crate::switcher::Mode::Windows, trigger);
            }
            KeyAction::SwitcherStep(delta) => self.step_switcher(delta),
            KeyAction::SwitcherCommit => self.commit_switcher(),
            KeyAction::SwitcherCancel => self.cancel_switcher(),
            KeyAction::SwitcherAppWindows => self.switcher_show_app_windows(),
            KeyAction::SwitcherCloseApp => self.switcher_close_app(),
            KeyAction::SwitcherCloseWindow => self.switcher_close_window(),
            KeyAction::SwitcherHide => self.switcher_hide_app(),
            KeyAction::CloseApp => self.close_focused_app(),
            KeyAction::Lock => self.lock_screen(),
            KeyAction::Logout => {
                self.open_power_dialog(Some(crate::power::PowerAction::LogOut));
            }
            KeyAction::PowerDialog => self.open_power_dialog(None),
            KeyAction::PowerStep(delta) => self.step_power_dialog(delta),
            KeyAction::PowerActivate => self.activate_power_dialog(),
            KeyAction::PowerCancel => self.cancel_power_dialog(),
            KeyAction::PickerCancel => self.cancel_picker(),
            KeyAction::ToggleOverview => {
                if self.layout.overview_active() {
                    self.begin_close_overview(None);
                } else {
                    self.open_overview();
                }
            }
            KeyAction::Exec(cmd) => {
                tracing::info!(command = %cmd, "executing keybind command");
                if let Err(err) = self.spawn_user_command(&cmd) {
                    tracing::error!(command = %cmd, ?err, "failed to execute keybind command");
                }
                crate::system_info::poke(crate::system_info::Topics::AUDIO | crate::system_info::Topics::BACKLIGHT);
            }
            KeyAction::ToggleLauncher => self.toggle_launcher(),
            KeyAction::LauncherPage(forward) => self.edit_launcher(|l| l.turn_page(forward)),
            KeyAction::LauncherNav(dx, dy) => self.edit_launcher(|l| l.move_selection(dx, dy)),
            KeyAction::LauncherActivate => {
                if let Some(entry) = self.launcher.as_ref().and_then(Launcher::selected_entry) {
                    self.activate_launcher_entry(entry);
                }
            }
            KeyAction::LauncherArrow(dx) => self.edit_launcher(|l| l.arrow(dx)),
            KeyAction::LauncherCaret(movement) => self.edit_launcher(|l| l.move_caret(movement)),
            KeyAction::LauncherDelete => self.edit_launcher(Launcher::delete_forward),
            KeyAction::LauncherDeleteWord => self.edit_launcher(Launcher::delete_word),
            KeyAction::LauncherSelectAll => self.edit_launcher(Launcher::select_all),
            KeyAction::LauncherPaste => self.paste_into_launcher(),
            KeyAction::Polkit(input) => self.polkit_key(input),
            KeyAction::ReplyType(ch) => {
                if self.notifications.type_reply(ch) {
                    self.queue_redraw();
                }
            }
            KeyAction::ReplyBackspace => {
                if self.notifications.backspace_reply() {
                    self.queue_redraw();
                }
            }
            KeyAction::ReplyCancel => {
                if self.notifications.cancel_reply() {
                    self.queue_redraw();
                }
            }
            KeyAction::ReplySend => {
                if self.notifications.send_reply() {
                    self.refresh_topbar();
                    self.queue_redraw();
                }
            }
            KeyAction::LauncherType(ch) => self.edit_launcher(|l| l.push_query(ch)),
            KeyAction::LauncherBackspace => self.edit_launcher(Launcher::pop_query),
            KeyAction::LauncherClear => {
                let menu_closed = self.launcher.as_mut().is_some_and(Launcher::close_menu);
                let cleared = menu_closed || self.launcher.as_mut().is_some_and(Launcher::clear_query);
                if cleared {
                    self.queue_redraw();
                } else {
                    self.toggle_launcher();
                }
            }
            KeyAction::Screenshot => self.open_shot(None),
            KeyAction::ScreenshotScreen => self.request_screenshot(),
            KeyAction::ScreenshotWindow => self.screenshot_window_now(),
            KeyAction::Shot(key) => self.shot_key(key),
            KeyAction::TopbarClosePopup => {
                if self.topbar.qs_escape() {
                    self.queue_redraw();
                    return;
                }
                let topbar_closed = self.topbar.close_popup();
                let dock_closed = self.dock.close_menu();
                if topbar_closed || dock_closed {
                    self.queue_redraw();
                }
            }
            KeyAction::TopbarNav(dx, dy) => {
                let adjusted = if dx != 0 && self.topbar.has_focus() {
                    let delta = i8::try_from(dx * 5).unwrap_or(0);
                    self.topbar.focus_adjust(delta)
                } else {
                    None
                };
                if let Some(action) = adjusted {
                    self.run_topbar_action(action);
                    self.queue_redraw();
                } else if self.topbar.focus_nav(dx, dy) {
                    self.queue_redraw();
                }
            }
            KeyAction::TopbarActivate => {
                if let Some(action) = self.topbar.focused_action() {
                    self.run_topbar_action(action);
                    self.queue_redraw();
                }
            }
            KeyAction::QsBack => {
                if self.topbar.close_detail() {
                    self.queue_redraw();
                }
            }
            KeyAction::QsField(key) => self.run_qs_field_key(key),
            KeyAction::GreeterType(ch) => {
                if self.greeter.as_mut().is_some_and(|g| g.type_char(ch)) {
                    self.queue_redraw();
                }
            }
            KeyAction::GreeterBackspace => {
                if self.greeter.as_mut().is_some_and(crate::greeter::GreeterUi::backspace) {
                    self.queue_redraw();
                }
            }
            KeyAction::GreeterSubmit => {
                if !self.confirm_greeter_power() {
                    self.submit_greeter_password();
                }
            }
            KeyAction::LockedPowerKey => self.locked_power_key(),
            KeyAction::ShowDesktop => self.toggle_show_desktop(),
            KeyAction::ForceQuit => self.force_quit_focused(),
            KeyAction::FillPassword => self.offer_passwords_now(),
            KeyAction::ScreenMirroring => self.toggle_screen_mirroring(),
            KeyAction::RestoreShortcuts => self.restore_shortcuts(),
            KeyAction::RunPrompt => self.open_run_prompt(),
            KeyAction::FocusDock
            | KeyAction::FocusTopBar
            | KeyAction::DockNav(_)
            | KeyAction::DockOpenMenu
            | KeyAction::DockActivate
            | KeyAction::DockLeave
            | KeyAction::DockMenuNav(_)
            | KeyAction::DockMenuActivate => self.run_shell_focus_action(&action),
            KeyAction::Tile(side) => self.tile_focused(side),
            KeyAction::TileMaximize => self.maximize_focused(),
            KeyAction::TileRestore => self.restore_focused(),
            KeyAction::MoveToOutput(side) => self.move_focused_to_output(side),
            KeyAction::Terminal => {
                let terminal = crate::global_menu::terminal_command();
                if let Err(err) = self.spawn_user_command(&terminal) {
                    tracing::error!(terminal, ?err, "failed to open a terminal");
                }
            }
            KeyAction::Shell(action) => self.run_topbar_action(action),
            KeyAction::Media(key) => self.run_media_key(key),
            KeyAction::MoveWindowToNeighbour(forward) => {
                if let Some(focused) = self.layout.focused_window() {
                    let target = if forward {
                        glide_layout::WorkspaceTarget::Next
                    } else {
                        glide_layout::WorkspaceTarget::Previous
                    };
                    tracing::info!(focused = ?focused, forward, "moving window to a neighbouring workspace");
                    self.suppressed_keys.clear();
                    self.layout.move_window_to_workspace(focused, target);
                    self.reconcile_space();
                    self.sync_ext_workspaces();
                }
            }
            KeyAction::GreeterCancel => {
                if self.greeter.as_mut().is_some_and(crate::greeter::GreeterUi::cancel) {
                    self.queue_redraw();
                }
            }
            KeyAction::GreeterCycleUser => self.greeter_cycle_user(),
            KeyAction::GreeterToggleLayout => {
                self.cycle_keyboard_layout();
                self.sync_greeter_layout();
                self.queue_redraw();
            }
            KeyAction::None => {}
        }
    }

    fn edit_launcher(&mut self, edit: impl FnOnce(&mut Launcher) -> bool) {
        if self.launcher.as_mut().is_some_and(edit) {
            self.queue_redraw();
        }
    }

    fn switch_workspace_by_key(&mut self, target: glide_layout::WorkspaceTarget) {
        self.suppressed_keys.clear();
        self.switch_workspace_animated(target);
        self.sync_ext_workspaces();
    }

    fn navigate_overview(&mut self, dir: Direction) {
        if self.overview.as_mut().is_some_and(|ui| ui.navigate(dir)) {
            self.queue_redraw();
        }
    }

    fn launcher_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let open = self.launcher.as_ref().is_some_and(|launcher| !launcher.is_closing());
        if !open {
            return false;
        }
        let area = self.launcher_area();
        let hit = self.launcher.as_ref().and_then(|launcher| launcher.hit_at(area, pos));
        let Some(launcher) = self.launcher.as_mut() else {
            return true;
        };
        let over_clear = launcher.clear_contains(area, pos);
        let chip = launcher.category_at(pos);
        let menu_changed = launcher.set_menu_hover(pos);
        let hover_changed = launcher.set_hover(hit);
        let clear_changed = launcher.set_clear_hover(over_clear);
        let chip_changed = launcher.set_chip_hover(chip);
        if menu_changed || hover_changed || clear_changed || chip_changed {
            self.queue_redraw();
        }
        true
    }

    fn topbar_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.session_lock.is_some() {
            return false;
        }
        if self.topbar_suppressed_on(self.output_rect()) && !self.topbar.floats() {
            if self.topbar.set_hover(None) {
                self.queue_redraw();
            }
            return false;
        }
        if let Some(drag_slot) = self.active_slider_drag
            && let Some(rect) = self.topbar.slot_rect(drag_slot)
        {
            let ratio = ((pos.x - f64::from(rect.loc.x)) / f64::from(rect.size.w)).clamp(0.0, 1.0);
            let pct = (ratio * 100.0).round() as u8;
            match drag_slot {
                crate::topbar::Slot::QsVolumeTrack => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsVolumeAdjust(pct));
                }
                crate::topbar::Slot::QsMicTrack => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsMicAdjust(pct));
                }
                crate::topbar::Slot::QsBrightnessTrack => {
                    self.handle_qs_action(crate::topbar::TopbarAction::QsBrightnessAdjust(pct));
                }
                _ => {}
            }
            self.queue_redraw();
            return true;
        }
        let output = self.output_rect();
        let over = self.topbar.contains(output, pos);
        if self.topbar.set_hover(over.then_some(pos)) {
            self.notifications.hold(self.topbar.held_notice());
            self.queue_redraw();
        }
        over
    }

    fn dock_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        if self.session_lock.is_some() {
            return false;
        }
        if !self.dock.is_enabled() || self.layout.overview_active() || self.dock_suppressed() {
            if self.dock.set_hover(None) {
                self.queue_redraw();
            }
            return false;
        }
        let area = self.dock_area();
        if self.dock.drag_motion(pos) {
            self.queue_redraw();
            return true;
        }
        let over = self.dock.contains(area, pos);
        if !over
            && self.dock.preview_open()
            && let Some(layout) = self.dock.preview_layout(area).filter(|layout| layout.keeps(pos))
        {
            let hover = self.dock.set_hover(None);
            let thumb = self.dock.set_preview_hover(layout.hit(pos));
            if hover || thumb {
                self.queue_redraw();
            }
            return true;
        }
        if self.dock.set_preview_hover(None) {
            self.queue_redraw();
        }
        let item = if over { self.dock.item_at(area, pos) } else { None };
        let keyboard = self.dock.keyboard_active();
        let tile_changed = (!keyboard || item.is_some()) && self.dock.set_hover(item);
        let on_menu = self.dock.menu_rect().is_some_and(|rect| rect.to_f64().contains(pos));
        let menu_changed = (!keyboard || on_menu) && self.dock.set_menu_hover(over.then_some(pos));
        if tile_changed || menu_changed {
            self.queue_redraw();
        }
        over
    }

    fn overview_motion(
        &mut self, pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        let under = if self.session_lock.is_some() { self.surface_under(pos) } else { self.shell_motion(pos) };
        let grabbed = self.seat.get_pointer().is_some_and(|pointer| pointer.is_grabbed());
        if under.is_none() && !grabbed {
            self.cursor_status =
                self.shell_cursor.map_or_else(CursorImageStatus::default_named, CursorImageStatus::Named);
        }
        under
    }

    fn shell_text_field_at(&self, pos: Point<f64, Logical>) -> bool {
        if let Some(greeter) = self.greeter.as_ref() {
            return greeter.hit_test(pos) == crate::greeter::GreeterHit::PasswordField;
        }
        let launcher_field = self.launcher.as_ref().filter(|launcher| !launcher.is_closing()).is_some_and(|launcher| {
            let area = self.launcher_area();
            launcher.search_contains(area, pos) && !launcher.clear_contains(area, pos)
        });
        let qs_field =
            self.topbar.has_open_popup() && self.topbar.slot_at(pos) == Some(crate::topbar::Slot::QsPasswordField);
        launcher_field || qs_field
    }

    fn shell_motion(
        &mut self, pos: Point<f64, Logical>,
    ) -> Option<(crate::focus::PointerFocusTarget, Point<f64, Logical>)> {
        self.shell_cursor = self.shell_text_field_at(pos).then_some(smithay::input::pointer::CursorIcon::Text);
        if self.shot_motion(pos) {
            self.clear_shell_hover();
            return None;
        }
        if self.picker_motion(pos) || self.power_dialog_motion(pos) {
            self.clear_shell_hover();
            return None;
        }
        if self.switcher_motion(pos) {
            self.clear_shell_hover();
            return None;
        }
        if self.greeter_motion(pos) {
            self.clear_shell_hover();
            return None;
        }
        let over_launcher = self.launcher_motion(pos);
        let over_topbar = self.topbar_motion(pos);
        let over_dock = self.dock_motion(pos);
        if over_dock || over_launcher || over_topbar {
            return None;
        }
        if !self.layout.overview_active() {
            return self.surface_under(pos);
        }

        let under = self.surface_under(pos);
        let on_layer = matches!(
            under,
            Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                if self.layer_for_surface(surface).is_some()
        );

        let on_strip = self.overview.as_ref().and_then(|ui| ui.workspace_at(pos));
        let on_close = self.overview.as_ref().filter(|_| on_strip.is_none()).and_then(|ui| ui.close_button_at(pos));
        let hovered =
            self.overview.as_ref().filter(|_| on_strip.is_none()).and_then(|ui| ui.preview_at(pos)).or(on_close);

        let changed = self.overview.as_mut().is_some_and(|ui| {
            let ring = ui.set_hover(hovered);
            let button = ui.set_close_hover(on_close);
            let thumb = ui.set_strip_hover(on_strip);
            ring || button || thumb
        });
        if changed {
            self.queue_redraw();
        }

        if on_layer { under } else { None }
    }

    fn clear_shell_hover(&mut self) {
        let topbar = self.topbar.set_hover(None);
        self.notifications.hold(None);
        let keyboard = self.dock.keyboard_active();
        let dock = !keyboard && self.dock.set_hover(None);
        let dock_menu = !keyboard && self.dock.set_menu_hover(None);
        if topbar || dock || dock_menu {
            self.queue_redraw();
        }
    }

    fn hot_corner_motion(&mut self, pos: Point<f64, Logical>) {
        let outputs: Vec<Rectangle<i32, Logical>> =
            self.layout.outputs().iter().map(|o| crate::state::logical_rect(o.geometry)).collect();
        let entered = self.hot_corner.update(pos, &outputs);
        if !entered || !self.overview_theme.hot_corner || self.screen_locked() {
            return;
        }
        let busy = self.seat.get_pointer().is_some_and(|pointer| pointer.is_grabbed());
        let fullscreen = self
            .layout
            .focused_window()
            .and_then(|id| self.layout.window_state(id))
            .is_some_and(|window| window.fullscreen);
        if busy || fullscreen {
            return;
        }
        if self.layout.overview_active() {
            self.begin_close_overview(None);
        } else {
            self.open_overview();
        }
        self.queue_redraw();
    }

    fn handle_pointer_motion<I: InputBackend>(&mut self, event: &I::PointerMotionEvent) {
        self.queue_redraw();
        let pointer = self.seat.get_pointer().expect("a pointer was added in GlideState::new");
        let mut pointer_location = pointer.current_location();
        let serial = SERIAL_COUNTER.next_serial();

        let under = self.overview_motion(pointer_location);

        let mut pointer_locked = false;
        let mut pointer_confined = false;
        let mut confine_region = None;
        if let Some((surface, surface_loc)) =
            under.as_ref().and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), *loc)))
        {
            with_pointer_constraint(&surface, &pointer, |constraint| {
                let Some(constraint) = constraint.filter(|c| c.is_active()) else {
                    return;
                };
                let in_region = constraint
                    .region()
                    .is_none_or(|region| region.contains((pointer_location - surface_loc).to_i32_round()));
                if !in_region {
                    return;
                }
                match &*constraint {
                    PointerConstraint::Locked(_) => pointer_locked = true,
                    PointerConstraint::Confined(confine) => {
                        pointer_confined = true;
                        confine_region = confine.region().cloned();
                    }
                }
            });
        }

        pointer.relative_motion(
            self,
            under.clone(),
            &RelativeMotionEvent { delta: event.delta(), delta_unaccel: event.delta_unaccel(), time: event.time() },
        );

        if pointer_locked {
            pointer.frame(self);
            return;
        }

        let mut delta = self.clamp_coords(pointer_location + event.delta()) - pointer_location;

        if pointer_confined && let (Some((_, surface_loc)), Some(region)) = (&under, &confine_region) {
            if !region.contains((pointer_location + Point::from((delta.x, 0.0)) - *surface_loc).to_i32_round()) {
                delta.x = 0.0;
            }
            if !region.contains((pointer_location + Point::from((0.0, delta.y)) - *surface_loc).to_i32_round()) {
                delta.y = 0.0;
            }
        }

        pointer_location += delta;

        let new_under = self.overview_motion(pointer_location);

        if pointer_confined
            && let Some((surface, _)) = &under
            && new_under.as_ref().and_then(|(target, _)| target.wl_surface()) != surface.wl_surface()
        {
            pointer.frame(self);
            return;
        }

        tracing::trace!(
            delta = ?event.delta(),
            location = ?pointer_location,
            "pointer motion (relative)"
        );

        self.flush_smooth_wheel_before(new_under.as_ref().and_then(|(target, _)| target.wl_surface()).as_deref());
        pointer.motion(
            self,
            new_under.clone(),
            &MotionEvent { location: pointer_location, serial, time: event.time() },
        );
        pointer.frame(self);
        self.hot_corner_motion(pointer_location);
        self.drag_assist_motion(pointer_location);

        if let Some((surface, surface_location)) =
            new_under.and_then(|(target, loc)| Some((target.wl_surface()?.into_owned(), loc)))
        {
            with_pointer_constraint(&surface, &pointer, |constraint| {
                let Some(constraint) = constraint.filter(|c| !c.is_active()) else {
                    return;
                };
                let point = (pointer_location - surface_location).to_i32_round();
                if constraint.region().is_none_or(|region| region.contains(point)) {
                    constraint.activate();
                }
            });
        }
    }

    fn handle_pointer_motion_absolute<I: InputBackend>(&mut self, event: &I::PointerMotionAbsoluteEvent) {
        self.queue_redraw();
        let Some(output) = self.space.outputs().next() else {
            return;
        };
        let output_geo =
            self.space.output_geometry(output).expect("output came from space.outputs(), so it has a geometry");

        let pos = event.position_transformed(output_geo.size) + output_geo.loc.to_f64();

        let serial = SERIAL_COUNTER.next_serial();

        let pointer = self.seat.get_pointer().expect("a pointer was added in GlideState::new");

        let under = self.overview_motion(pos);

        self.flush_smooth_wheel_before(under.as_ref().and_then(|(target, _)| target.wl_surface()).as_deref());
        pointer.motion(self, under, &MotionEvent { location: pos, serial, time: event.time() });
        pointer.frame(self);
        self.hot_corner_motion(pos);
        self.drag_assist_motion(pos);
    }

    #[allow(clippy::too_many_lines)]
    fn handle_pointer_button<I: InputBackend>(&mut self, event: &I::PointerButtonEvent) {
        let pointer = self.seat.get_pointer().expect("a pointer was added in GlideState::new");
        let serial = SERIAL_COUNTER.next_serial();

        let button = event.button_code();

        let button_state = event.state();

        if button_state == ButtonState::Released {
            let shot_released = self.shot_release(pointer.current_location());
            let picker_released = self.picker_release(pointer.current_location());
            if shot_released || picker_released {
                pointer.frame(self);
                return;
            }
            self.shell_release(pointer.current_location(), button);
        }

        if ButtonState::Pressed == button_state && !pointer.is_grabbed() {
            let pos = pointer.current_location();

            self.session.super_tap.interrupt();
            if self.session_lock.is_none() && self.dismiss_window_menus(pointer.current_focus().as_ref()) {
                pointer.frame(self);
                return;
            }
            if self.session_lock.is_some() {
                self.focus_lock_surface();
            } else if self.shell_button(pos, button) {
                pointer.frame(self);
                return;
            } else {
                self.focus_click_target(pos);
                if self.super_drag(pos, button, serial) {
                    pointer.button(self, &ButtonEvent { button, state: button_state, serial, time: event.time() });
                    pointer.frame(self);
                    return;
                }
            }
        }

        pointer.button(self, &ButtonEvent { button, state: button_state, serial, time: event.time() });
        pointer.frame(self);
    }

    fn dismiss_window_menus(&mut self, target: Option<&crate::focus::PointerFocusTarget>) -> bool {
        let mut dismissed = false;
        for window in self.space.elements() {
            let own = matches!(target, Some(crate::focus::PointerFocusTarget::Decoration(deco)) if deco.0 == *window);
            if !own && let Some(mut deco) = window.try_decoration_state_mut() {
                dismissed |= deco.header.dismiss_menu();
            }
        }
        if dismissed {
            self.queue_redraw();
        }
        dismissed
    }

    fn shell_release(&mut self, pos: Point<f64, Logical>, button: u32) {
        self.active_slider_drag = None;
        self.topbar.set_slider_dragging(false);
        if self.topbar.set_pressed(None) {
            self.queue_redraw();
        }
        if button == BTN_LEFT {
            let area = self.dock_area();
            match self.dock.end_press(area, pos) {
                crate::dock::PressOutcome::Click(index) => self.activate_dock_item(index),
                crate::dock::PressOutcome::Drop { index, target } => {
                    self.drop_dock_tile(index, target);
                }
                crate::dock::PressOutcome::Nothing => {}
            }
        }
    }

    fn shell_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        if self.handoff_blocks_input() {
            return true;
        }
        if self.greeter_button(pos, button) {
            return true;
        }
        if self.polkit_focused() {
            let output = self.output_rect();
            if self.topbar.contains(output, pos) {
                self.topbar_button(pos, button);
            }
            return true;
        }
        if self.shot_button(pos, button) {
            return true;
        }
        if self.picker_button(pos) || self.power_dialog_button(pos) {
            return true;
        }
        if self.switcher_button(pos) {
            return true;
        }
        if self.topbar_button(pos, button) {
            return true;
        }
        if self.launcher_button(pos, button) {
            return true;
        }
        if self.dock_button(pos, button) {
            return true;
        }
        self.overview_button(pos, button)
    }

    fn greeter_motion(&mut self, pos: Point<f64, Logical>) -> bool {
        let Some(ref mut greeter) = self.greeter else {
            return false;
        };
        let hit = greeter.hit_test(pos);
        if greeter.update_hover(hit) {
            self.queue_redraw();
        }
        true
    }

    fn greeter_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        use crate::greeter::GreeterHit;
        let Some(greeter) = self.greeter.as_mut() else {
            return false;
        };
        if button != BTN_LEFT {
            return true;
        }
        let hit = greeter.hit_test(pos);
        match hit {
            GreeterHit::Submit => self.submit_greeter_password(),
            GreeterHit::UserAvatar => self.greeter_cycle_user(),
            GreeterHit::User(index) => self.greeter_select_user(index),
            GreeterHit::Session => greeter.toggle_session_menu(),
            GreeterHit::SessionItem(index) => greeter.select_session(index),
            GreeterHit::PowerButton => greeter.toggle_power_menu(),
            GreeterHit::Power(action) if greeter.armed() == Some(action) => self.greeter_power(action),
            GreeterHit::Power(action) => greeter.arm_power(action),
            GreeterHit::KeyboardLayout => {
                self.cycle_keyboard_layout();
                self.sync_greeter_layout();
            }
            GreeterHit::Media(button) => {
                let command = match button {
                    crate::greeter::MediaButton::Previous => glide_media::Command::Previous,
                    crate::greeter::MediaButton::PlayPause => glide_media::Command::PlayPause,
                    crate::greeter::MediaButton::Next => glide_media::Command::Next,
                };
                self.run_media_command(command);
            }
            GreeterHit::Menu | GreeterHit::Status => {}
            GreeterHit::PasswordField | GreeterHit::None => {
                greeter.close_menus();
            }
        }
        self.queue_redraw();
        true
    }

    fn topbar_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        let output = self.output_rect();
        if (self.topbar.floats() || !self.topbar_suppressed_on(output)) && self.topbar.contains(output, pos) {
            if let Some(mut action) = self.topbar.action_at(pos) {
                if let crate::topbar::TopbarAction::Tray(click) = &mut action {
                    click.button = crate::topbar::TrayButton::from_code(button);
                }
                let slot = self.topbar.slot_at(pos);
                if let Some(slot) = slot
                    && slot.is_slider()
                {
                    self.active_slider_drag = Some(slot);
                    self.topbar.set_slider_dragging(true);
                }
                if self.topbar.set_pressed(slot) {
                    self.queue_redraw();
                }
                self.run_topbar_action(action);
            }
            return true;
        }
        if self.topbar.close_popup() {
            self.queue_redraw();
            return true;
        }
        false
    }

    fn launcher_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        let area = self.launcher_area();
        let Some(launcher) = self.launcher.as_mut().filter(|l| !l.is_closing()) else {
            return false;
        };
        if let (Some(item), Some(result)) = (launcher.menu_item_at(pos), launcher.menu_result()) {
            launcher.close_menu();
            self.run_launcher_menu_item(result, item);
            self.queue_redraw();
            return true;
        }
        if launcher.close_menu() {
            self.queue_redraw();
            return true;
        }

        if launcher.clear_contains(area, pos) {
            if button != BTN_RIGHT && launcher.clear_text() {
                launcher.set_clear_hover(false);
                self.queue_redraw();
            }
            return true;
        }
        if launcher.is_listing() {
            let entry = launcher.entry_at(area, pos).and_then(|index| launcher.entries().get(index).cloned());
            let on_search = launcher.search_contains(area, pos);
            match entry {
                Some(entry) if button != BTN_RIGHT => self.activate_launcher_entry(entry),
                Some(_) => {}
                None if on_search || button == BTN_RIGHT => {}
                None => {
                    launcher.begin_close();
                    self.queue_redraw();
                }
            }
            return true;
        }

        let hit = launcher.app_at(area, pos);
        if button == BTN_RIGHT {
            if let Some(result) = hit {
                let pinned = self.launcher_app_is_pinned(result);
                let opened = self.launcher.as_mut().is_some_and(|launcher| launcher.open_menu(result, area, pinned));
                if opened {
                    self.queue_redraw();
                }
            }
            return true;
        }

        let dot = launcher.dot_at(area, pos);
        let on_search = launcher.search_contains(area, pos);
        let chip = launcher.category_at(pos);
        let changed = if let Some(result) = hit {
            self.activate_launcher_app(result);
            false
        } else if let Some(category) = chip {
            launcher.toggle_category(category)
        } else if let Some(page) = dot {
            launcher.go_to_page(page)
        } else if on_search {
            false
        } else {
            launcher.begin_close();
            true
        };
        if changed {
            self.queue_redraw();
        }
        true
    }

    fn dock_preview_button(&mut self, area: Rectangle<i32, Logical>, pos: Point<f64, Logical>, button: u32) -> bool {
        use crate::dock_preview::PreviewHit;
        if !self.dock.preview_open() {
            return false;
        }
        let Some(layout) = self.dock.preview_layout(area).filter(|layout| layout.keeps(pos)) else {
            return false;
        };
        if !layout.panel.to_f64().contains(pos) && self.dock.contains(area, pos) {
            return false;
        }
        let index = self.dock.preview_index();
        match (layout.hit(pos), button) {
            (Some(PreviewHit::Close(id) | PreviewHit::Window(id)), BTN_MIDDLE) | (Some(PreviewHit::Close(id)), _) => {
                self.send_close_to(glide_layout::WindowId::from_u64(id));
            }
            (Some(PreviewHit::Window(id)), BTN_LEFT) => {
                self.dock.hide_preview();
                if let Some(index) = index {
                    self.run_dock_menu_item(index, crate::dock::DockMenuItem::Window(id));
                }
            }
            _ => {}
        }
        self.queue_redraw();
        true
    }

    fn dock_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        if !self.dock.is_enabled() || self.layout.overview_active() || self.dock_suppressed() {
            return false;
        }
        let area = self.dock_area();
        if self.dock_preview_button(area, pos, button) {
            return true;
        }
        if let Some(item) = self.dock.menu_item_at(pos) {
            let index = self.dock.menu_index();
            self.dock.close_menu();
            self.dock.end_keyboard();
            if let Some(index) = index {
                self.run_dock_menu_item(index, item);
            }
            self.queue_redraw();
            return true;
        }
        let menu_closed = self.dock.close_menu();
        let left_keyboard = self.dock.end_keyboard();
        if menu_closed || left_keyboard {
            self.queue_redraw();
        }
        if !self.dock.contains(area, pos) {
            return menu_closed;
        }
        if let Some(index) = self.dock.item_at(area, pos) {
            if button == BTN_RIGHT {
                let context = self.dock_menu_context(index);
                if self.dock.open_menu(index, area, &context) {
                    self.queue_redraw();
                }
            } else if button == BTN_MIDDLE {
                self.run_dock_menu_item(index, crate::dock::DockMenuItem::NewWindow);
            } else if self.dock.action(index) == Some(crate::dock::DockAction::OpenLauncher) {
                self.toggle_launcher();
            } else {
                self.dock.begin_press(index, pos, area);
            }
        }
        true
    }

    fn overview_button(&mut self, pos: Point<f64, Logical>, button: u32) -> bool {
        if !self.layout.overview_active() {
            return false;
        }
        let on_layer = matches!(
            self.surface_under(pos),
            Some((crate::focus::PointerFocusTarget::Surface(ref surface), _))
                if self.layer_for_surface(surface).is_some()
        );
        if on_layer {
            return false;
        }

        if let Some(workspace) = self.overview.as_ref().and_then(|ui| ui.workspace_at(pos)) {
            self.overview_switch_workspace(workspace);
            return true;
        }
        let close = self.overview.as_ref().and_then(|ui| ui.close_button_at(pos));
        if let Some(id) = close {
            self.close_from_overview(id);
            return true;
        }

        match self.overview.as_ref().and_then(|ui| ui.preview_at(pos)) {
            Some(id) if button == BTN_MIDDLE => self.close_from_overview(id),
            hit => {
                self.begin_close_overview(hit);
            }
        }
        true
    }

    fn focus_click_target(&mut self, pos: Point<f64, Logical>) {
        let target = self.surface_under(pos).map(|(target, _)| target);
        match target {
            Some(crate::focus::PointerFocusTarget::Surface(surface)) if self.layer_for_surface(&surface).is_some() => {
                let layer = self.layer_for_surface(&surface).expect("matched Some in the guard above");
                if layer.can_receive_keyboard_focus() {
                    self.layer_focus = Some(layer);
                    self.apply_keyboard_focus();
                }
            }
            Some(crate::focus::PointerFocusTarget::Decoration(deco)) => {
                if let Some(id) = self.window_id(&deco.0) {
                    self.focus_window(id);
                }
            }
            Some(crate::focus::PointerFocusTarget::Surface(_)) => {
                let clicked = self.space.element_under(pos).map(|(window, _)| window.clone());
                if let Some(id) = clicked.as_ref().and_then(|window| self.window_id(window)) {
                    self.focus_window(id);
                }
            }
            None => self.clear_focus(),
        }
    }

    fn topbar_scroll<I: InputBackend>(&mut self, event: &I::PointerAxisEvent) -> bool {
        if self.screen_locked() || self.topbar_suppressed_on(self.output_rect()) {
            return false;
        }
        let pos = self.seat.get_pointer().expect("a pointer was added in GlideState::new").current_location();
        let Some(slot) = self.topbar.slot_at(pos) else {
            return self.topbar.contains(self.output_rect(), pos);
        };
        let delta = scroll_delta::<I>(event);
        if let crate::topbar::Slot::Tray(id) = slot {
            self.scroll_tray(id, delta);
            return true;
        }
        let steps = self.topbar.scroll_steps(delta);
        if steps == 0 {
            return true;
        }
        if matches!(slot, crate::topbar::Slot::NcItem(_) | crate::topbar::Slot::NcItemDismiss(_)) {
            if self.topbar.scroll_notifications(steps) {
                self.queue_redraw();
            }
            return true;
        }
        let step = i16::try_from(steps.saturating_mul(-5).clamp(-200, 200)).unwrap_or(0);
        let status = self.topbar.system_status();
        let action = match slot {
            crate::topbar::Slot::Status | crate::topbar::Slot::QsVolumeMute | crate::topbar::Slot::QsVolumeTrack => {
                let pct = (i16::from(status.volume_pct) + step).clamp(0, i16::from(self.max_volume_pct));
                Some(crate::topbar::TopbarAction::QsVolumeAdjust(u8::try_from(pct).unwrap_or(0)))
            }
            crate::topbar::Slot::QsMicMute | crate::topbar::Slot::QsMicTrack => {
                let pct = (i16::from(status.mic_pct) + step).clamp(0, 100);
                Some(crate::topbar::TopbarAction::QsMicAdjust(u8::try_from(pct).unwrap_or(0)))
            }
            crate::topbar::Slot::QsBrightnessTrack => {
                let pct = (i16::from(status.brightness_pct) + step).clamp(1, 100);
                Some(crate::topbar::TopbarAction::QsBrightnessAdjust(u8::try_from(pct).unwrap_or(1)))
            }
            _ => None,
        };
        if let Some(action) = action {
            self.handle_qs_action(action);
            self.queue_redraw();
        }
        true
    }

    fn dock_scroll<I: InputBackend>(&mut self, event: &I::PointerAxisEvent) -> bool {
        if self.screen_locked()
            || !self.dock.is_enabled()
            || self.dock.is_hidden()
            || self.launcher.is_some()
            || self.layout.overview_active()
            || self.dock_suppressed()
        {
            return false;
        }
        let pos = self.seat.get_pointer().expect("a pointer was added in GlideState::new").current_location();
        let area = self.dock_area();
        if !self.dock.contains(area, pos) {
            return false;
        }
        let Some(index) = self.dock.item_at(area, pos) else {
            return true;
        };
        let steps = self.dock.scroll_steps(scroll_delta::<I>(event));
        if steps != 0 {
            self.cycle_app_windows(index, steps);
        }
        true
    }

    fn handle_pointer_axis<I: InputBackend>(&mut self, event: &I::PointerAxisEvent) {
        if self.greeter.is_some()
            || self.shot_active()
            || self.picker_active()
            || self.topbar_scroll::<I>(event)
            || self.dock_scroll::<I>(event)
        {
            return;
        }
        if self.launcher.as_ref().is_some_and(|launcher| !launcher.is_closing()) {
            let vertical = axis_amount::<I>(event, Axis::Vertical);
            let horizontal = axis_amount::<I>(event, Axis::Horizontal);
            let amount = if vertical == 0.0 { horizontal } else { vertical };
            let finger = event.source() == AxisSource::Finger;
            let Some(launcher) = self.launcher.as_mut() else {
                return;
            };
            if amount == 0.0 {
                if finger {
                    launcher.end_scroll();
                }
            } else if launcher.scroll_by(amount, finger) {
                self.queue_redraw();
            }
            return;
        }

        let source = event.source();
        let factor =
            self.input_config.pointer_settings(crate::config::is_touchpad(&Event::device(event))).scroll_factor;
        let notches = [Axis::Horizontal, Axis::Vertical]
            .map(|axis| (event.amount_v120(axis).unwrap_or(0.0), axis_amount::<I>(event, axis)));
        if self.smooth_wheel_takes(source, notches, factor) {
            return;
        }

        let mut frame = AxisFrame::new(event.time()).source(source);
        for axis in [Axis::Horizontal, Axis::Vertical] {
            let amount = factor * axis_amount::<I>(event, axis);
            if amount != 0.0 {
                frame = frame.value(axis, amount);
                if let Some(discrete) = event.amount_v120(axis).map(|value| value * factor) {
                    {
                        frame = frame.v120(axis, discrete as i32);
                    }
                }
            }
        }
        if source == AxisSource::Finger {
            for axis in [Axis::Horizontal, Axis::Vertical] {
                if event.amount(axis) == Some(0.0) {
                    frame = frame.stop(axis);
                }
            }
        }

        let pointer = self.seat.get_pointer().expect("a pointer was added in GlideState::new");
        pointer.axis(self, frame);
        pointer.frame(self);
    }

    fn clamp_coords(&self, pos: Point<f64, Logical>) -> Point<f64, Logical> {
        let output_rects: Vec<Rectangle<i32, Logical>> = self
            .space
            .outputs()
            .map(|o| self.space.output_geometry(o).expect("output came from space.outputs(), so it has a geometry"))
            .collect();
        clamp_to_outputs(pos, &output_rects)
    }
}

#[cfg(test)]
mod tests {
    use smithay::{input::keyboard::ModifiersState, utils::Point};

    #[test]
    fn touch_lands_on_the_internal_panel_even_when_it_is_not_first() {
        let names = |list: &[&str]| list.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
        assert_eq!(super::touch_output_index(&names(&["DP-1", "eDP-1"])), Some(1));
        assert_eq!(super::touch_output_index(&names(&["HDMI-A-1", "DP-2"])), Some(0));
        assert_eq!(super::touch_output_index(&names(&["DSI-1"])), Some(0));
        assert_eq!(super::touch_output_index(&[]), None);
    }

    use super::*;

    use crate::raster::rect;

    fn no_modifiers() -> ModifiersState {
        ModifiersState::default()
    }

    #[test]
    fn keyboard_leds_follow_the_xkb_lock_state_and_unknown_leds_stay_dark() {
        use smithay::{input::keyboard::LedState, reexports::input::Led};
        let state = |num, caps, scroll| LedState { num, caps, scroll };
        assert_eq!(libinput_leds(state(Some(true), Some(false), None)), Led::NUMLOCK);
        assert_eq!(libinput_leds(state(Some(true), Some(true), Some(true))), Led::all());
        assert_eq!(libinput_leds(state(None, Some(true), None)), Led::CAPSLOCK);
        assert_eq!(libinput_leds(LedState::default()), Led::empty());
    }

    #[test]
    fn keypad_navigation_works_in_shell_fields_while_num_lock_is_off() {
        for (keypad, plain) in [
            (Keysym::KP_Left, Keysym::Left),
            (Keysym::KP_End, Keysym::End),
            (Keysym::KP_Next, Keysym::Next),
            (Keysym::KP_Delete, Keysym::Delete),
        ] {
            assert_eq!(keypad_navigation(keypad), plain);
        }
        assert_eq!(keypad_navigation(Keysym::KP_4), Keysym::KP_4, "digits stay digits");
        let raw = [Keysym::KP_Home];
        let caret = launcher_active_shortcut(&[], &no_modifiers(), &raw, keypad_navigation(Keysym::KP_Home));
        assert_eq!(caret, KeyAction::LauncherCaret(crate::launcher::field::CaretMove::Home));
    }

    #[test]
    fn keypad_keys_type_digits_while_num_lock_is_on_although_their_raw_level_is_navigation() {
        let plain = no_modifiers();
        for (raw, digit, ch) in [
            (Keysym::KP_Home, Keysym::KP_7, '7'),
            (Keysym::KP_Up, Keysym::KP_8, '8'),
            (Keysym::KP_Left, Keysym::KP_4, '4'),
            (Keysym::KP_Begin, Keysym::KP_5, '5'),
            (Keysym::KP_Right, Keysym::KP_6, '6'),
            (Keysym::KP_End, Keysym::KP_1, '1'),
            (Keysym::KP_Next, Keysym::KP_3, '3'),
            (Keysym::KP_Insert, Keysym::KP_0, '0'),
            (Keysym::KP_Delete, Keysym::KP_Decimal, '.'),
        ] {
            let sym = keypad_navigation(digit);
            assert_eq!(launcher_active_shortcut(&[], &plain, &[raw], sym), KeyAction::LauncherType(ch), "{digit:?}");
            assert_eq!(greeter_active_shortcut(&plain, &[raw], sym), KeyAction::GreeterType(ch), "{digit:?}");
            let matches = |want: Keysym| sym == want || raw == want;
            let field = crate::topbar::field_key(false, true, sym.key_char(), matches);
            assert_eq!(field, Some(crate::topbar::FieldKey::Type(ch)), "{digit:?}");
        }
    }

    mod launcher_keys {
        use super::*;

        fn typed(sym: Keysym) -> KeyAction {
            launcher_active_shortcut(&[], &no_modifiers(), &[sym], sym)
        }

        fn with(mods: ModifiersState, sym: Keysym) -> KeyAction {
            launcher_active_shortcut(&[], &mods, &[sym], sym)
        }

        #[test]
        fn printable_keys_type_into_the_query_and_editing_keys_keep_their_meaning() {
            use crate::launcher::field::CaretMove;
            for (sym, ch) in
                [(Keysym::a, 'a'), (Keysym::z, 'z'), (Keysym::_7, '7'), (Keysym::minus, '-'), (Keysym::space, ' ')]
            {
                assert_eq!(typed(sym), KeyAction::LauncherType(ch));
            }
            for (sym, action) in [
                (Keysym::BackSpace, KeyAction::LauncherBackspace),
                (Keysym::Escape, KeyAction::LauncherClear),
                (Keysym::Return, KeyAction::LauncherActivate),
                (Keysym::Right, KeyAction::LauncherArrow(1)),
                (Keysym::Left, KeyAction::LauncherArrow(-1)),
                (Keysym::Down, KeyAction::LauncherNav(0, 1)),
                (Keysym::Tab, KeyAction::LauncherNav(1, 0)),
                (Keysym::Delete, KeyAction::LauncherDelete),
                (Keysym::Home, KeyAction::LauncherCaret(CaretMove::Home)),
                (Keysym::End, KeyAction::LauncherCaret(CaretMove::End)),
            ] {
                assert_eq!(typed(sym), action, "{sym:?}");
            }
        }

        #[test]
        fn modifiers_select_capitals_word_edits_or_leave_the_key_to_global_bindings() {
            use crate::launcher::field::CaretMove;
            let mods = |shift, ctrl, alt, logo| ModifiersState { shift, ctrl, alt, logo, ..ModifiersState::default() };
            let (shift, ctrl) = (mods(true, false, false, false), mods(false, true, false, false));
            assert_eq!(with(shift, Keysym::A), KeyAction::LauncherType('A'));
            assert_eq!(with(shift, Keysym::Tab), KeyAction::LauncherNav(-1, 0));
            for (sym, action) in [
                (Keysym::BackSpace, KeyAction::LauncherDeleteWord),
                (Keysym::Left, KeyAction::LauncherCaret(CaretMove::WordLeft)),
                (Keysym::Right, KeyAction::LauncherCaret(CaretMove::WordRight)),
                (Keysym::a, KeyAction::LauncherSelectAll),
                (Keysym::v, KeyAction::LauncherPaste),
            ] {
                assert_eq!(with(ctrl, sym), action, "{sym:?}");
            }
            assert_ne!(with(mods(false, true, false, true), Keysym::v), KeyAction::LauncherPaste);
            for combo in [ctrl, mods(false, false, true, false), mods(false, false, false, true)] {
                assert_eq!(with(combo, Keysym::b), KeyAction::None);
            }
        }
    }

    mod clamp_to_outputs {
        use super::*;

        #[test]
        fn positions_are_pulled_to_the_nearest_output_pixel_and_left_alone_inside() {
            let pos = Point::from((-100.0, 500.0));
            assert_eq!(clamp_to_outputs(pos, &[]), pos, "no outputs, no clamping");
            let single = [rect(0, 0, 1920, 1080)];
            let stacked = [rect(0, 0, 1920, 1080), rect(0, 1080, 1920, 1080)];
            let mixed = [rect(0, 0, 1920, 1080), rect(1920, 0, 1280, 720)];
            for (outputs, from, to) in [
                (&single[..], (500.0, 500.0), (500.0, 500.0)),
                (&single[..], (-50.0, 500.0), (0.0, 500.0)),
                (&single[..], (5000.0, 500.0), (1919.0, 500.0)),
                (&single[..], (500.0, 5000.0), (500.0, 1079.0)),
                (&stacked[..], (500.0, 1500.0), (500.0, 1500.0)),
                (&mixed[..], (2500.0, 5000.0), (1919.0, 1079.0)),
            ] {
                let result = clamp_to_outputs(Point::from(from), outputs);
                assert_eq!((result.x, result.y), to, "{from:?}");
                assert!(outputs.iter().any(|r| r.to_f64().contains(result)));
            }
            let gapped = [rect(0, 0, 1000, 1080), rect(2000, 0, 1000, 1080)];
            let result = clamp_to_outputs(Point::from((1500.0, 5000.0)), &gapped);
            let nudged: Point<f64, Logical> = Point::from((result.x - 0.5, result.y - 0.5));
            assert!(gapped.iter().any(|r| r.to_f64().contains(result) || r.to_f64().contains(nudged)));
        }
    }

    fn default_binds() -> Vec<ResolvedKeybind> {
        crate::config::Config::with_defaults().resolve_keybinds()
    }

    mod process_keyboard_shortcut {
        use super::*;

        fn mods(logo: bool, shift: bool, alt: bool) -> ModifiersState {
            ModifiersState { logo, shift, alt, ..ModifiersState::default() }
        }

        #[test]
        fn the_default_bindings_resolve_on_the_raw_keysym() {
            let binds = default_binds();
            let none = no_modifiers();
            let (logo, logo_shift, alt) = (mods(true, false, false), mods(true, true, false), mods(false, false, true));
            let vt = |n| Keysym::from(keysyms::KEY_XF86Switch_VT_1 + n - 1);
            for (modifiers, raw, sym, action) in [
                (logo, None, Keysym::q, Some(KeyAction::CloseApp)),
                (logo_shift, Some(Keysym::q), Keysym::Q, Some(KeyAction::Logout)),
                (none, None, Keysym::q, None),
                (alt, None, Keysym::grave, Some(KeyAction::SwitchAppWindows(false))),
                (
                    mods(false, true, true),
                    Some(Keysym::grave),
                    Keysym::asciitilde,
                    Some(KeyAction::SwitchAppWindows(true)),
                ),
                (alt, None, Keysym::F4, Some(KeyAction::CloseFocused)),
                (none, None, Keysym::F4, None),
                (none, None, vt(2), Some(KeyAction::VtSwitch(2))),
                (none, None, vt(12), Some(KeyAction::VtSwitch(12))),
                (none, None, Keysym::a, None),
                (logo, None, Keysym::from(keysyms::KEY_1), Some(KeyAction::SwitchWorkspace(0))),
                (logo, None, Keysym::from(keysyms::KEY_9), Some(KeyAction::SwitchWorkspace(8))),
                (logo_shift, None, Keysym::from(keysyms::KEY_3), Some(KeyAction::MoveWindowToWorkspace(2))),
                (
                    logo_shift,
                    Some(Keysym::from(keysyms::KEY_3)),
                    Keysym::numbersign,
                    Some(KeyAction::MoveWindowToWorkspace(2)),
                ),
                (logo, None, Keysym::Tab, Some(KeyAction::NextWorkspace)),
                (logo_shift, None, Keysym::Tab, Some(KeyAction::PrevWorkspace)),
                (logo_shift, Some(Keysym::Tab), Keysym::ISO_Left_Tab, Some(KeyAction::PrevWorkspace)),
            ] {
                let raw: Vec<Keysym> = raw.into_iter().collect();
                assert_eq!(process_keyboard_shortcut(&binds, &modifiers, &raw, sym), action, "{sym:?}");
            }
            assert_eq!(
                process_keyboard_shortcut(&[], &none, &[], vt(3)),
                Some(KeyAction::VtSwitch(3)),
                "VT switching works with no keybinds at all"
            );
        }

        #[test]
        fn an_explicit_quit_binding_still_parses() {
            let config: crate::config::Config =
                toml::from_str("[[keybinds]]\nmods = [\"super\", \"ctrl\"]\nkey = \"q\"\naction = \"quit\"\n")
                    .expect("valid keybind toml");
            let binds = config.resolve_keybinds();
            let mut modifiers = no_modifiers();
            modifiers.logo = true;
            modifiers.ctrl = true;

            let action = process_keyboard_shortcut(&binds, &modifiers, &[], Keysym::q);

            assert_eq!(action, Some(KeyAction::Quit));
        }

        #[test]
        fn inside_the_switcher_grave_drills_into_windows_and_qwh_act_on_the_selection() {
            let mut modifiers = no_modifiers();
            modifiers.alt = true;
            let apps = crate::switcher::Mode::Apps;
            let windows = crate::switcher::Mode::Windows;
            let key = |sym, mode| switcher_active_shortcut(&modifiers, &[sym], sym, mode);

            assert_eq!(key(Keysym::grave, apps), KeyAction::SwitcherAppWindows);
            assert_eq!(key(Keysym::grave, windows), KeyAction::SwitcherStep(1));
            assert_eq!(key(Keysym::q, apps), KeyAction::SwitcherCloseApp);
            assert_eq!(key(Keysym::w, windows), KeyAction::SwitcherCloseWindow);
            assert_eq!(key(Keysym::h, apps), KeyAction::SwitcherHide);
            assert_eq!(key(Keysym::Escape, apps), KeyAction::SwitcherCancel);
        }

        #[test]
        fn the_power_dialog_swallows_every_key_but_its_own_and_vt_switch() {
            let plain = no_modifiers();
            let key = |sym| power_dialog_shortcut(&plain, &[sym], sym);
            assert_eq!(key(Keysym::Escape), KeyAction::PowerCancel);
            assert_eq!(key(Keysym::Return), KeyAction::PowerActivate);
            assert_eq!(key(Keysym::space), KeyAction::PowerActivate);
            assert_eq!(key(Keysym::Right), KeyAction::PowerStep(1));
            assert_eq!(key(Keysym::Tab), KeyAction::PowerStep(1));
            assert_eq!(key(Keysym::Left), KeyAction::PowerStep(-1));
            assert_eq!(key(Keysym::ISO_Left_Tab), KeyAction::PowerStep(-1));
            assert_eq!(key(Keysym::q), KeyAction::None);
            assert_eq!(key(Keysym::XF86_Switch_VT_2), KeyAction::VtSwitch(2));
            let mut shift = no_modifiers();
            shift.shift = true;
            assert_eq!(power_dialog_shortcut(&shift, &[Keysym::Tab], Keysym::Tab), KeyAction::PowerStep(-1));
        }
    }

    #[test]
    fn only_escape_is_taken_by_the_dock_menu() {
        let none = no_modifiers();
        assert_eq!(dock_menu_shortcut(&[], &none, &[], Keysym::Escape, false), Some(KeyAction::TopbarClosePopup));
        for sym in [Keysym::Return, Keysym::Left, Keysym::Down, Keysym::Tab, Keysym::space] {
            assert_eq!(dock_menu_shortcut(&[], &none, &[], sym, false), None);
        }
    }

    #[test]
    fn a_menu_opened_from_the_keyboard_is_walked_with_arrows() {
        let none = no_modifiers();
        let key = |sym| dock_menu_shortcut(&[], &none, &[], sym, true);
        assert_eq!(key(Keysym::Down), Some(KeyAction::DockMenuNav(1)));
        assert_eq!(key(Keysym::Up), Some(KeyAction::DockMenuNav(-1)));
        assert_eq!(key(Keysym::Return), Some(KeyAction::DockMenuActivate));
        assert_eq!(key(Keysym::Escape), Some(KeyAction::TopbarClosePopup));
    }

    #[test]
    fn the_focused_dock_walks_opens_activates_and_lets_go() {
        let none = no_modifiers();
        let shift = ModifiersState { shift: true, ..ModifiersState::default() };
        let key = |modifiers: &ModifiersState, sym| dock_keys_shortcut(&[], modifiers, &[], sym);
        assert_eq!(key(&none, Keysym::Right), Some(KeyAction::DockNav(1)));
        assert_eq!(key(&none, Keysym::Left), Some(KeyAction::DockNav(-1)));
        assert_eq!(key(&shift, Keysym::Tab), Some(KeyAction::DockNav(-1)));
        assert_eq!(key(&none, Keysym::Up), Some(KeyAction::DockOpenMenu));
        assert_eq!(key(&shift, Keysym::F10), Some(KeyAction::DockOpenMenu));
        assert_eq!(key(&none, Keysym::Return), Some(KeyAction::DockActivate));
        assert_eq!(key(&none, Keysym::Escape), Some(KeyAction::DockLeave));
        assert_eq!(key(&none, Keysym::a), None, "other keys still reach the focused window");
        assert!(keeps_dock_focus(&KeyAction::DockNav(1)));
        assert!(!keeps_dock_focus(&KeyAction::ToggleLauncher), "any other shortcut lets the dock go");
    }

    #[test]
    fn ctrl_f2_and_ctrl_f3_reach_the_top_bar_and_the_dock() {
        let binds = default_binds();
        let ctrl = ModifiersState { ctrl: true, ..ModifiersState::default() };
        let action = |sym| process_keyboard_shortcut(&binds, &ctrl, &[sym], sym);
        assert_eq!(action(Keysym::F2), Some(KeyAction::FocusTopBar));
        assert_eq!(action(Keysym::F3), Some(KeyAction::FocusDock));
    }

    mod overview_active_shortcut_tests {
        use super::*;

        #[test]
        fn the_overview_keeps_its_own_keys_vt_switch_and_screenshots_but_nothing_else() {
            let binds = default_binds();
            let (none, logo) = (no_modifiers(), ModifiersState { logo: true, ..ModifiersState::default() });
            for (modifiers, sym, action) in [
                (none, Keysym::Escape, KeyAction::OverviewCancel),
                (none, Keysym::from(keysyms::KEY_XF86Switch_VT_4), KeyAction::VtSwitch(4)),
                (none, Keysym::Print, KeyAction::Screenshot),
                (none, Keysym::space, KeyAction::OverviewSelect),
                (none, Keysym::a, KeyAction::None),
                (logo, Keysym::q, KeyAction::None),
                (logo, Keysym::space, KeyAction::None),
            ] {
                assert_eq!(overview_active_shortcut(&binds, &modifiers, &[], sym), action, "{sym:?}");
            }
        }

        #[test]
        fn the_overview_and_launcher_toggles_still_fire_during_overview() {
            let logo = ModifiersState { logo: true, ..ModifiersState::default() };
            let binds = [
                ResolvedKeybind { modifiers: logo, keysym: Keysym::w, action: KeyAction::ToggleOverview },
                ResolvedKeybind { modifiers: logo, keysym: Keysym::a, action: KeyAction::ToggleLauncher },
            ];
            assert_eq!(overview_active_shortcut(&binds, &logo, &[], Keysym::w), KeyAction::ToggleOverview);
            assert_eq!(overview_active_shortcut(&binds, &logo, &[], Keysym::a), KeyAction::ToggleLauncher);
        }
    }

    #[test]
    fn the_greeter_types_submits_cycles_and_still_switches_layouts_and_vts() {
        let plain = no_modifiers();
        let logo = ModifiersState { logo: true, ..ModifiersState::default() };
        for (modifiers, sym, action) in [
            (plain, Keysym::a, KeyAction::GreeterType('a')),
            (plain, Keysym::_1, KeyAction::GreeterType('1')),
            (plain, Keysym::Return, KeyAction::GreeterSubmit),
            (plain, Keysym::BackSpace, KeyAction::GreeterBackspace),
            (plain, Keysym::Escape, KeyAction::GreeterCancel),
            (plain, Keysym::Tab, KeyAction::GreeterCycleUser),
            (logo, Keysym::space, KeyAction::GreeterToggleLayout),
        ] {
            assert_eq!(greeter_active_shortcut(&modifiers, &[sym], sym), action, "{sym:?}");
        }
        let vt = Keysym::from(keysyms::KEY_XF86Switch_VT_1);
        assert_eq!(greeter_active_shortcut(&plain, &[], vt), KeyAction::VtSwitch(1));
        assert_eq!(greeter_active_shortcut(&plain, &[], Keysym::XF86_PowerOff), KeyAction::LockedPowerKey);
    }

    #[test]
    fn a_character_that_wakes_the_lock_screen_is_typed_but_other_keys_only_wake_it() {
        let plain = no_modifiers();
        let shift = ModifiersState { shift: true, ..ModifiersState::default() };
        let ctrl = ModifiersState { ctrl: true, ..ModifiersState::default() };
        assert_eq!(wake_typed_char(&plain, Keysym::p), Some(KeyAction::GreeterType('p')));
        assert_eq!(wake_typed_char(&shift, Keysym::P), Some(KeyAction::GreeterType('P')));
        for sym in [Keysym::Return, Keysym::space, Keysym::Escape, Keysym::BackSpace, Keysym::Shift_L] {
            assert_eq!(wake_typed_char(&plain, sym), None, "{sym:?}");
        }
        assert_eq!(wake_typed_char(&ctrl, Keysym::p), None);
    }

    #[test]
    fn the_power_key_and_lock_reach_the_shell_from_every_overlay() {
        let binds = default_binds();
        let (none, logo) = (no_modifiers(), ModifiersState { logo: true, ..ModifiersState::default() });
        let power = Keysym::XF86_PowerOff;
        assert_eq!(overview_active_shortcut(&binds, &none, &[], power), KeyAction::PowerDialog);
        assert_eq!(launcher_active_shortcut(&binds, &none, &[], power), KeyAction::PowerDialog);
        assert_eq!(reply_active_shortcut(&binds, &none, &[], power), KeyAction::PowerDialog);
        assert_eq!(launcher_active_shortcut(&binds, &logo, &[], Keysym::l), KeyAction::Lock);
        assert_eq!(overview_active_shortcut(&binds, &logo, &[], Keysym::l), KeyAction::Lock);
        assert_eq!(
            overview_active_shortcut(&binds, &none, &[], Keysym::l),
            KeyAction::OverviewNav(Direction::Right),
            "an unmodified l still walks the grid"
        );
    }

    #[test]
    fn super_escape_alone_takes_the_shortcuts_back() {
        let logo = ModifiersState { logo: true, ..ModifiersState::default() };
        let logo_ctrl = ModifiersState { ctrl: true, ..logo };
        assert!(restores_shortcuts(&logo, &[Keysym::Escape], Keysym::Escape));
        assert!(!restores_shortcuts(&no_modifiers(), &[Keysym::Escape], Keysym::Escape));
        assert!(!restores_shortcuts(&logo_ctrl, &[Keysym::Escape], Keysym::Escape));
        assert!(!restores_shortcuts(&logo, &[Keysym::q], Keysym::q));
    }

    #[test]
    fn only_media_keys_bound_to_commands_work_while_locked() {
        let mut binds = default_binds();
        let logo = ModifiersState { logo: true, ..ModifiersState::default() };
        binds.push(ResolvedKeybind {
            modifiers: logo,
            keysym: Keysym::Return,
            action: KeyAction::Exec("terminal".into()),
        });
        let none = no_modifiers();
        let locked = |modifiers: &ModifiersState, sym| locked_media_shortcut(&binds, modifiers, &[sym], sym);
        for sym in [
            Keysym::XF86_AudioRaiseVolume,
            Keysym::XF86_AudioMute,
            Keysym::XF86_MonBrightnessDown,
            Keysym::XF86_KbdBrightnessUp,
        ] {
            assert!(matches!(locked(&none, sym), Some(KeyAction::Exec(_))), "{sym:?}");
        }
        for (sym, key) in [
            (Keysym::XF86_AudioPlay, MediaKey::PlayPause),
            (Keysym::XF86_AudioPause, MediaKey::Pause),
            (Keysym::XF86_AudioStop, MediaKey::Stop),
            (Keysym::XF86_AudioNext, MediaKey::Next),
            (Keysym::XF86_AudioPrev, MediaKey::Previous),
            (Keysym::XF86_AudioForward, MediaKey::Forward),
            (Keysym::XF86_AudioRewind, MediaKey::Rewind),
        ] {
            assert_eq!(
                locked(&none, sym),
                Some(KeyAction::Media(key)),
                "headset buttons work behind the lock: {sym:?}"
            );
        }
        assert_eq!(locked(&logo, Keysym::Return), None, "a launcher binding must not run behind the lock");
        assert_eq!(locked(&logo, Keysym::q), None);
        assert_eq!(locked(&none, Keysym::XF86_PowerOff), None);
        assert_eq!(locked(&none, Keysym::XF86_Search), None, "the launcher stays behind the lock");
    }

    #[test]
    fn media_actions_are_named_and_old_playerctl_bindings_turn_into_them() {
        assert_eq!(MediaKey::from_name("media-play-pause"), Some(MediaKey::PlayPause));
        assert_eq!(MediaKey::from_name("media-rewind"), Some(MediaKey::Rewind));
        assert_eq!(MediaKey::from_name("media-volume-up"), None);
        assert_eq!(MediaKey::from_playerctl("playerctl play-pause"), Some(MediaKey::PlayPause));
        assert_eq!(MediaKey::from_playerctl("  playerctl   next "), Some(MediaKey::Next));
        assert_eq!(MediaKey::from_playerctl("playerctl previous"), Some(MediaKey::Previous));
        assert_eq!(
            MediaKey::from_playerctl("playerctl --player=spotify next"),
            None,
            "targeted commands stay commands"
        );
        assert_eq!(MediaKey::from_playerctl("playerctl position 10+"), None);
    }

    #[test]
    fn the_new_shell_actions_have_default_bindings() {
        use crate::topbar::TopbarAction;
        let binds = default_binds();
        let logo = ModifiersState { logo: true, ..ModifiersState::default() };
        let logo_shift = ModifiersState { logo: true, shift: true, ..ModifiersState::default() };
        for (modifiers, sym, action) in [
            (logo, Keysym::d, KeyAction::ShowDesktop),
            (logo, Keysym::v, KeyAction::Shell(TopbarAction::ToggleNotificationCenter)),
            (logo, Keysym::s, KeyAction::Shell(TopbarAction::ToggleQuickSettings)),
            (logo_shift, Keysym::Page_Down, KeyAction::MoveWindowToNeighbour(true)),
            (logo_shift, Keysym::Page_Up, KeyAction::MoveWindowToNeighbour(false)),
        ] {
            assert_eq!(process_keyboard_shortcut(&binds, &modifiers, &[], sym), Some(action), "{sym:?}");
        }
        let config: crate::config::Config = toml::from_str(
            "[[keybinds]]\nmods = [\"super\"]\nkey = \"n\"\naction = \"toggle-do-not-disturb\"\n\
             [[keybinds]]\nmods = [\"super\"]\nkey = \"c\"\naction = \"toggle-caffeine\"\n",
        )
        .expect("valid keybind toml");
        let actions: Vec<KeyAction> = config.resolve_keybinds().into_iter().map(|bind| bind.action).collect();
        assert_eq!(
            actions,
            [KeyAction::Shell(TopbarAction::ToggleDoNotDisturb), KeyAction::Shell(TopbarAction::ToggleCaffeine)]
        );
    }

    #[test]
    fn a_shortcut_inhibitor_cannot_swallow_the_power_key() {
        assert!(survives_inhibit(&KeyAction::PowerDialog, true));
        assert!(survives_inhibit(&KeyAction::VtSwitch(2), false));
        assert!(!survives_inhibit(&KeyAction::Lock, false));
        assert!(!survives_inhibit(&KeyAction::Exec("true".into()), false));
        assert!(is_power_key(&[Keysym::XF86_PowerOff], Keysym::NoSymbol));
        assert!(!is_power_key(&[Keysym::q], Keysym::q));
    }

    #[test]
    fn layout_switch_keys_are_intercepted_without_typing_space_or_activating_popups() {
        let logo = ModifiersState { logo: true, ..ModifiersState::default() };
        assert!(intercepts_layout_switch(&[Keysym::space], Keysym::ISO_Next_Group));
        assert!(intercepts_layout_switch(&[Keysym::ISO_Next_Group], Keysym::ISO_Next_Group));
        assert!(!intercepts_layout_switch(&[Keysym::Shift_L], Keysym::ISO_Next_Group));
        assert!(!intercepts_layout_switch(&[Keysym::space], Keysym::space));
        assert_eq!(power_dialog_shortcut(&logo, &[Keysym::space], Keysym::ISO_Next_Group), KeyAction::None);
        assert_eq!(power_dialog_shortcut(&logo, &[Keysym::space], Keysym::space), KeyAction::None);
        assert_eq!(
            topbar_popup_shortcut(
                &[],
                &logo,
                &[Keysym::space],
                Keysym::ISO_Next_Group,
                crate::topbar::QsKeyMode::Typing
            ),
            None
        );
        assert_eq!(
            topbar_popup_shortcut(&[], &logo, &[Keysym::space], Keysym::space, crate::topbar::QsKeyMode::Normal),
            None
        );
        assert_eq!(
            greeter_active_shortcut(&no_modifiers(), &[Keysym::ISO_Next_Group], Keysym::ISO_Next_Group),
            KeyAction::GreeterToggleLayout
        );
    }
}
