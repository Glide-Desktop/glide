use alacritty_terminal::term::TermMode;
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{Key, Modifiers};

#[must_use]
pub fn latin(physical: Physical) -> Option<char> {
    let Physical::Code(code) = physical else { return None };
    let letters = [
        (Code::KeyA, 'a'),
        (Code::KeyB, 'b'),
        (Code::KeyC, 'c'),
        (Code::KeyD, 'd'),
        (Code::KeyE, 'e'),
        (Code::KeyF, 'f'),
        (Code::KeyG, 'g'),
        (Code::KeyH, 'h'),
        (Code::KeyI, 'i'),
        (Code::KeyJ, 'j'),
        (Code::KeyK, 'k'),
        (Code::KeyL, 'l'),
        (Code::KeyM, 'm'),
        (Code::KeyN, 'n'),
        (Code::KeyO, 'o'),
        (Code::KeyP, 'p'),
        (Code::KeyQ, 'q'),
        (Code::KeyR, 'r'),
        (Code::KeyS, 's'),
        (Code::KeyT, 't'),
        (Code::KeyU, 'u'),
        (Code::KeyV, 'v'),
        (Code::KeyW, 'w'),
        (Code::KeyX, 'x'),
        (Code::KeyY, 'y'),
        (Code::KeyZ, 'z'),
        (Code::Digit0, '0'),
        (Code::Digit1, '1'),
        (Code::Digit2, '2'),
        (Code::Digit3, '3'),
        (Code::Digit4, '4'),
        (Code::Digit5, '5'),
        (Code::Digit6, '6'),
        (Code::Digit7, '7'),
        (Code::Digit8, '8'),
        (Code::Digit9, '9'),
        (Code::Minus, '-'),
        (Code::Equal, '='),
        (Code::BracketLeft, '['),
        (Code::BracketRight, ']'),
        (Code::Backslash, '\\'),
        (Code::Semicolon, ';'),
        (Code::Quote, '\''),
        (Code::Comma, ','),
        (Code::Period, '.'),
        (Code::Slash, '/'),
        (Code::Backquote, '`'),
        (Code::Space, ' '),
    ];
    letters.into_iter().find(|(known, _)| *known == code).map(|(_, letter)| letter)
}

#[must_use]
pub fn character(key: &Key, physical: Physical) -> Option<char> {
    match key {
        Key::Character(text) => {
            let typed = text.chars().next()?.to_ascii_lowercase();
            if typed.is_ascii() { Some(typed) } else { latin(physical) }
        }
        Key::Named(Named::Space) => Some(' '),
        _ => None,
    }
}

fn modifier_code(modifiers: Modifiers) -> u8 {
    1 + modifiers.shift() as u8 + 2 * modifiers.alt() as u8 + 4 * modifiers.control() as u8
}

fn csi(number: &str, modifiers: Modifiers, last: char) -> Vec<u8> {
    let code = modifier_code(modifiers);
    if code == 1 {
        format!("\x1b[{number}{last}").into_bytes()
    } else {
        let number = if number.is_empty() { "1" } else { number };
        format!("\x1b[{number};{code}{last}").into_bytes()
    }
}

fn cursor_key(letter: char, modifiers: Modifiers, mode: TermMode) -> Vec<u8> {
    if modifier_code(modifiers) == 1 && mode.contains(TermMode::APP_CURSOR) {
        format!("\x1bO{letter}").into_bytes()
    } else {
        csi("", modifiers, letter)
    }
}

fn function_key(number: u8, modifiers: Modifiers) -> Vec<u8> {
    match number {
        1..=4 if modifier_code(modifiers) == 1 => format!("\x1bO{}", char::from(b'O' + number)).into_bytes(),
        1..=4 => csi("1", modifiers, char::from(b'O' + number)),
        _ => {
            let code = [15, 17, 18, 19, 20, 21, 23, 24][usize::from(number - 5)];
            csi(&code.to_string(), modifiers, '~')
        }
    }
}

fn named(key: Named, modifiers: Modifiers, mode: TermMode) -> Option<Vec<u8>> {
    let alt = |bytes: &[u8]| {
        let mut out = if modifiers.alt() { vec![0x1b] } else { Vec::new() };
        out.extend_from_slice(bytes);
        out
    };
    Some(match key {
        Named::Enter => alt(b"\r"),
        Named::Tab if modifiers.shift() => b"\x1b[Z".to_vec(),
        Named::Tab => alt(b"\t"),
        Named::Backspace if modifiers.control() => alt(b"\x08"),
        Named::Backspace => alt(b"\x7f"),
        Named::Escape => alt(b"\x1b"),
        Named::Space if modifiers.control() => alt(b"\0"),
        Named::Space => alt(b" "),
        Named::ArrowUp => cursor_key('A', modifiers, mode),
        Named::ArrowDown => cursor_key('B', modifiers, mode),
        Named::ArrowRight => cursor_key('C', modifiers, mode),
        Named::ArrowLeft => cursor_key('D', modifiers, mode),
        Named::Home => cursor_key('H', modifiers, mode),
        Named::End => cursor_key('F', modifiers, mode),
        Named::Insert => csi("2", modifiers, '~'),
        Named::Delete => csi("3", modifiers, '~'),
        Named::PageUp => csi("5", modifiers, '~'),
        Named::PageDown => csi("6", modifiers, '~'),
        Named::F1 => function_key(1, modifiers),
        Named::F2 => function_key(2, modifiers),
        Named::F3 => function_key(3, modifiers),
        Named::F4 => function_key(4, modifiers),
        Named::F5 => function_key(5, modifiers),
        Named::F6 => function_key(6, modifiers),
        Named::F7 => function_key(7, modifiers),
        Named::F8 => function_key(8, modifiers),
        Named::F9 => function_key(9, modifiers),
        Named::F10 => function_key(10, modifiers),
        Named::F11 => function_key(11, modifiers),
        Named::F12 => function_key(12, modifiers),
        _ => return None,
    })
}

const fn control(letter: char) -> Option<u8> {
    Some(match letter {
        'a'..='z' => letter as u8 - b'a' + 1,
        '@' | ' ' | '2' => 0,
        '[' | '3' => 0x1b,
        '\\' | '4' => 0x1c,
        ']' | '5' => 0x1d,
        '^' | '6' | '~' => 0x1e,
        '_' | '7' | '-' | '/' => 0x1f,
        '8' | '?' => 0x7f,
        _ => return None,
    })
}

#[must_use]
pub fn encode(
    key: &Key, physical: Physical, modifiers: Modifiers, text: Option<&str>, mode: TermMode, alt_is_meta: bool,
) -> Option<Vec<u8>> {
    if let Key::Named(name) = key
        && let Some(bytes) = named(*name, modifiers, mode)
    {
        return Some(bytes);
    }
    let meta = modifiers.alt() && alt_is_meta;
    if modifiers.control()
        && let Some(byte) = character(key, physical).and_then(control)
    {
        return Some(if meta { vec![0x1b, byte] } else { vec![byte] });
    }
    let text = text.filter(|text| !text.is_empty() && !text.chars().any(char::is_control))?;
    let mut bytes = if meta { vec![0x1b] } else { Vec::new() };
    if meta && let Key::Character(typed) = key {
        bytes.extend_from_slice(typed.as_bytes());
    } else {
        bytes.extend_from_slice(text.as_bytes());
    }
    Some(bytes)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Button {
    Left,
    Middle,
    Right,
    WheelUp,
    WheelDown,
    Moved(Option<u8>),
}

#[must_use]
pub fn mouse_report(
    mode: TermMode, button: Button, pressed: bool, column: usize, line: usize, modifiers: Modifiers,
) -> Option<Vec<u8>> {
    let base = match button {
        Button::Left => 0,
        Button::Middle => 1,
        Button::Right => 2,
        Button::WheelUp => 64,
        Button::WheelDown => 65,
        Button::Moved(held) => 32 + held.unwrap_or(3),
    };
    let modifiers =
        4 * u8::from(modifiers.shift()) + 8 * u8::from(modifiers.alt()) + 16 * u8::from(modifiers.control());
    let (column, line) = (column + 1, line + 1);
    if mode.contains(TermMode::SGR_MOUSE) {
        let end = if pressed { 'M' } else { 'm' };
        return Some(format!("\x1b[<{};{column};{line}{end}", base + modifiers).into_bytes());
    }
    let code = if pressed || matches!(button, Button::WheelUp | Button::WheelDown) { base } else { 3 };
    let mut out = vec![0x1b, b'[', b'M', 32 + code + modifiers];
    for position in [column, line] {
        if mode.contains(TermMode::UTF8_MOUSE) {
            let encoded = char::from_u32(u32::try_from(32 + position).ok()?)?;
            let mut buffer = [0; 4];
            out.extend_from_slice(encoded.encode_utf8(&mut buffer).as_bytes());
        } else {
            out.push(u8::try_from(32 + position).ok()?);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::key::NativeCode;

    const NONE: Modifiers = Modifiers::empty();

    fn press(key: &Key, modifiers: Modifiers, text: Option<&str>) -> Option<Vec<u8>> {
        encode(key, Physical::Unidentified(NativeCode::Unidentified), modifiers, text, TermMode::default(), true)
    }

    fn char_key(text: &str) -> Key {
        Key::Character(text.into())
    }

    #[test]
    fn space_and_printable_text_reach_the_shell() {
        assert_eq!(press(&Key::Named(Named::Space), NONE, Some(" ")), Some(b" ".to_vec()));
        assert_eq!(press(&char_key("ї"), NONE, Some("ї")), Some("ї".as_bytes().to_vec()));
        assert_eq!(press(&char_key("A"), Modifiers::SHIFT, Some("A")), Some(b"A".to_vec()));
    }

    #[test]
    fn control_keys_follow_the_physical_layout_on_cyrillic() {
        let key = char_key("с");
        let bytes = encode(&key, Physical::Code(Code::KeyC), Modifiers::CTRL, Some("с"), TermMode::default(), true);
        assert_eq!(bytes, Some(vec![3]));
        assert_eq!(press(&Key::Named(Named::Space), Modifiers::CTRL, None), Some(vec![0]));
        assert_eq!(press(&char_key("["), Modifiers::CTRL, None), Some(vec![0x1b]));
    }

    #[test]
    fn alt_prefixes_escape_when_meta() {
        assert_eq!(press(&char_key("b"), Modifiers::ALT, Some("b")), Some(b"\x1bb".to_vec()));
        assert_eq!(press(&Key::Named(Named::Backspace), Modifiers::ALT, None), Some(b"\x1b\x7f".to_vec()));
    }

    #[test]
    fn modified_navigation_keys_carry_xterm_parameters() {
        assert_eq!(press(&Key::Named(Named::ArrowLeft), Modifiers::CTRL, None), Some(b"\x1b[1;5D".to_vec()));
        assert_eq!(press(&Key::Named(Named::Home), NONE, None), Some(b"\x1b[H".to_vec()));
        assert_eq!(press(&Key::Named(Named::PageUp), Modifiers::SHIFT, None), Some(b"\x1b[5;2~".to_vec()));
        assert_eq!(press(&Key::Named(Named::F1), NONE, None), Some(b"\x1bOP".to_vec()));
        assert_eq!(press(&Key::Named(Named::F5), Modifiers::CTRL, None), Some(b"\x1b[15;5~".to_vec()));
        let app =
            encode(&Key::Named(Named::ArrowUp), Physical::Code(Code::ArrowUp), NONE, None, TermMode::APP_CURSOR, true);
        assert_eq!(app, Some(b"\x1bOA".to_vec()));
    }

    #[test]
    fn mouse_reports_use_sgr_when_asked_and_x10_otherwise() {
        let sgr = TermMode::SGR_MOUSE | TermMode::MOUSE_REPORT_CLICK;
        assert_eq!(mouse_report(sgr, Button::Left, true, 4, 1, NONE), Some(b"\x1b[<0;5;2M".to_vec()));
        assert_eq!(mouse_report(sgr, Button::Left, false, 4, 1, NONE), Some(b"\x1b[<0;5;2m".to_vec()));
        let x10 = mouse_report(TermMode::MOUSE_REPORT_CLICK, Button::Right, false, 0, 0, NONE);
        assert_eq!(x10, Some(vec![0x1b, b'[', b'M', 32 + 3, 33, 33]));
        assert_eq!(mouse_report(TermMode::MOUSE_REPORT_CLICK, Button::Left, true, 300, 0, NONE), None);
    }
}
