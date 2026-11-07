use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Modifiers};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command {
    PlayPause,
    Pause,
    Shuttle(bool),
    Skip(i32),
    Step(bool),
    Volume(f32),
    Mute,
    Fullscreen,
    Escape,
    Subtitles,
    NextText,
    NextAudio,
    Slower,
    Faster,
    NormalSpeed,
    Chapter(bool),
    Trim,
    Split,
    Export,
    Inspector,
    Pip,
    Open,
    OpenAddress,
    Shortcuts,
    MarkIn,
    MarkOut,
    Confirm,
    CopyFrame,
    TextOnFrame,
    RotateLeft,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Context {
    pub playing: bool,
    pub editing: bool,
    pub loaded: bool,
}

fn code(physical: Physical) -> Option<Code> {
    match physical {
        Physical::Code(code) => Some(code),
        Physical::Unidentified(_) => None,
    }
}

#[must_use]
pub fn command(key: &Key, physical: Physical, modifiers: Modifiers, context: Context) -> Option<Command> {
    let (ctrl, shift, alt) = (modifiers.control(), modifiers.shift(), modifiers.alt());
    if alt || modifiers.logo() {
        return None;
    }
    if let Key::Named(named) = key {
        let found = match (named, ctrl, shift) {
            (Named::Space, false, false) => Command::PlayPause,
            (Named::ArrowLeft, false, false) if !context.playing => Command::Step(false),
            (Named::ArrowRight, false, false) if !context.playing => Command::Step(true),
            (Named::ArrowLeft, false, shift) => Command::Skip(if shift { -30 } else { -5 }),
            (Named::ArrowRight, false, shift) => Command::Skip(if shift { 30 } else { 5 }),
            (Named::ArrowUp, false, false) => Command::Volume(0.05),
            (Named::ArrowDown, false, false) => Command::Volume(-0.05),
            (Named::Escape, false, false) => Command::Escape,
            (Named::Backspace, false, false) => Command::NormalSpeed,
            (Named::Delete, false, false) if context.editing => Command::Delete,
            (Named::Enter, false, false) if context.editing => Command::Confirm,
            (Named::F1, false, false) => Command::Shortcuts,
            (Named::F11, false, false) => Command::Fullscreen,
            _ => return None,
        };
        return (context.loaded || matches!(found, Command::Shortcuts | Command::Escape)).then_some(found);
    }
    let found = match (code(physical)?, ctrl, shift) {
        (Code::KeyO, true, false) => return Some(Command::Open),
        (Code::KeyL, true, false) => return Some(Command::OpenAddress),
        (Code::Slash, true, true) => return Some(Command::Shortcuts),
        (Code::KeyK, false, false) => Command::Pause,
        (Code::KeyJ, false, false) => Command::Shuttle(false),
        (Code::KeyL, false, false) => Command::Shuttle(true),
        (Code::KeyM, false, false) => Command::Mute,
        (Code::KeyF, false, false) => Command::Fullscreen,
        (Code::KeyC, false, false) => Command::Subtitles,
        (Code::KeyC, false, true) => Command::NextText,
        (Code::KeyA, false, false) => Command::NextAudio,
        (Code::BracketLeft, false, false) => Command::Slower,
        (Code::BracketRight, false, false) => Command::Faster,
        (Code::Comma, false, false) => Command::Chapter(false),
        (Code::Period, false, false) => Command::Chapter(true),
        (Code::KeyI, false, false) if context.editing => Command::MarkIn,
        (Code::KeyO, false, false) if context.editing => Command::MarkOut,
        (Code::KeyT, true, false) => Command::Trim,
        (Code::KeyY, true, false) => Command::Split,
        (Code::KeyE, true, false) => Command::Export,
        (Code::KeyI, true, false) => Command::Inspector,
        (Code::KeyP, true, true) => Command::Pip,
        (Code::KeyC, true, true) => Command::CopyFrame,
        (Code::KeyT, true, true) => Command::TextOnFrame,
        (Code::KeyL, true, true) => Command::RotateLeft,
        _ => return None,
    };
    context.loaded.then_some(found)
}

#[must_use]
pub fn shuttle(rate: f64, forward: bool) -> f64 {
    let speeds = [1.0, 2.0, 4.0, 8.0];
    let magnitude = rate.abs();
    let same_way = (rate >= 0.0) == forward;
    let next = if same_way && magnitude >= 1.0 {
        speeds.iter().copied().find(|speed| *speed > magnitude + 0.01).unwrap_or(8.0)
    } else {
        1.0
    };
    if forward { next } else { -next }
}

pub const SPEEDS: [f64; 6] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

#[must_use]
pub fn step_speed(rate: f64, faster: bool) -> f64 {
    let rate = rate.abs();
    if faster {
        SPEEDS.iter().copied().find(|speed| *speed > rate + 0.01).unwrap_or(2.0)
    } else {
        SPEEDS.iter().rev().copied().find(|speed| *speed < rate - 0.01).unwrap_or(0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn loaded(playing: bool) -> Context {
        Context { playing, editing: false, loaded: true }
    }

    fn letter(code: Code, text: &str) -> (Key, Physical) {
        (Key::Character(text.into()), Physical::Code(code))
    }

    #[test]
    fn letters_resolve_by_physical_key_on_other_layouts() {
        let (key, physical) = letter(Code::KeyK, "л");
        assert_eq!(command(&key, physical, Modifiers::empty(), loaded(true)), Some(Command::Pause));
        let (key, physical) = letter(Code::KeyT, "е");
        assert_eq!(command(&key, physical, Modifiers::CTRL, loaded(true)), Some(Command::Trim));
    }

    #[test]
    fn arrows_step_frames_on_pause_and_skip_while_playing() {
        let left = Key::Named(Named::ArrowLeft);
        let physical = Physical::Code(Code::ArrowLeft);
        assert_eq!(command(&left, physical, Modifiers::empty(), loaded(false)), Some(Command::Step(false)));
        assert_eq!(command(&left, physical, Modifiers::empty(), loaded(true)), Some(Command::Skip(-5)));
        assert_eq!(command(&left, physical, Modifiers::SHIFT, loaded(false)), Some(Command::Skip(-30)));
    }

    #[test]
    fn nothing_but_opening_works_without_a_file() {
        let empty = Context::default();
        let (key, physical) = letter(Code::KeyO, "o");
        assert_eq!(command(&key, physical, Modifiers::CTRL, empty), Some(Command::Open));
        let space = Key::Named(Named::Space);
        assert_eq!(command(&space, Physical::Code(Code::Space), Modifiers::empty(), empty), None);
    }

    #[test]
    fn in_and_out_marks_only_while_editing() {
        let (key, physical) = letter(Code::KeyI, "i");
        assert_eq!(command(&key, physical, Modifiers::empty(), loaded(false)), None);
        let editing = Context { editing: true, ..loaded(false) };
        assert_eq!(command(&key, physical, Modifiers::empty(), editing), Some(Command::MarkIn));
    }

    #[test]
    fn shuttle_speeds_double_and_reverse_resets() {
        assert_eq!(shuttle(1.0, true), 2.0);
        assert_eq!(shuttle(2.0, true), 4.0);
        assert_eq!(shuttle(8.0, true), 8.0);
        assert_eq!(shuttle(4.0, false), -1.0);
        assert_eq!(shuttle(-1.0, false), -2.0);
        assert_eq!(shuttle(0.5, true), 1.0);
    }

    #[test]
    fn speed_steps_walk_the_menu_values() {
        assert_eq!(step_speed(1.0, true), 1.25);
        assert_eq!(step_speed(2.0, true), 2.0);
        assert_eq!(step_speed(1.0, false), 0.75);
        assert_eq!(step_speed(0.5, false), 0.5);
    }
}
