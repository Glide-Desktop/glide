use std::time::Duration;

use glide_shell::i18n::Key;

pub const COUNTDOWN: Duration = Duration::from_mins(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PowerAction {
    Lock,
    LogOut,
    Sleep,
    Restart,
    ShutDown,
}

#[must_use]
pub const fn is_destructive(action: PowerAction) -> bool {
    matches!(action, PowerAction::LogOut | PowerAction::Restart | PowerAction::ShutDown)
}

#[must_use]
pub const fn question_key(action: PowerAction) -> Key {
    match action {
        PowerAction::Lock => Key::PowerAskLock,
        PowerAction::Sleep => Key::PowerAskSleep,
        PowerAction::Restart => Key::PowerAskRestart,
        PowerAction::ShutDown => Key::PowerAskShutDown,
        PowerAction::LogOut => Key::PowerAskLogOut,
    }
}

#[must_use]
pub const fn confirm_key(action: PowerAction) -> Key {
    match action {
        PowerAction::Lock => Key::PowerLock,
        PowerAction::Sleep => Key::PowerConfirmSleep,
        PowerAction::Restart => Key::PowerConfirmRestart,
        PowerAction::ShutDown => Key::CommandShutDown,
        PowerAction::LogOut => Key::PowerConfirmLogOut,
    }
}
