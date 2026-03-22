#![allow(dead_code)]

pub const DOOM_KEY_UP: u8 = 0xad;
pub const DOOM_KEY_DOWN: u8 = 0xaf;
pub const DOOM_KEY_LEFT: u8 = 0xac;
pub const DOOM_KEY_RIGHT: u8 = 0xae;
pub const DOOM_KEY_ESCAPE: u8 = 27;
pub const DOOM_KEY_ENTER: u8 = 13;
pub const DOOM_KEY_TAB: u8 = 9;
pub const DOOM_KEY_BACKSPACE: u8 = 0x7f;
pub const DOOM_KEY_MINUS: u8 = 0x2d;
pub const DOOM_KEY_EQUALS: u8 = 0x3d;
pub const DOOM_KEY_STRAFE_L: u8 = 0xa0;
pub const DOOM_KEY_STRAFE_R: u8 = 0xa1;
pub const DOOM_KEY_USE: u8 = 0xa2;
pub const DOOM_KEY_FIRE: u8 = 0xa3;

/// Abstract key symbols we care about, independent of backend.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeySymbol {
    Char(char),
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    Escape,
    Enter,
    Tab,
    Backspace,
    Home,
    End,
    PageUp,
    PageDown,
    Space,
    Minus,
    Equals,
}

/// Map a `KeySymbol` to the Doom scancode expected by doomgeneric.
pub fn scancode_from_symbol(symbol: KeySymbol) -> Option<u8> {
    use KeySymbol::*;
    match symbol {
        Char(c) => scancode_from_char(c),
        ArrowUp => Some(DOOM_KEY_UP),
        ArrowDown => Some(DOOM_KEY_DOWN),
        ArrowLeft => Some(DOOM_KEY_LEFT),
        ArrowRight => Some(DOOM_KEY_RIGHT),
        Escape => Some(DOOM_KEY_ESCAPE),
        Enter => Some(DOOM_KEY_ENTER),
        Tab => Some(DOOM_KEY_TAB),
        Backspace => Some(DOOM_KEY_BACKSPACE),
        Home => Some(b'H'),
        End => Some(b'F'),
        PageUp => Some(b'5'),
        PageDown => Some(b'6'),
        Space => Some(DOOM_KEY_USE),
        Minus => Some(DOOM_KEY_MINUS),
        Equals => Some(DOOM_KEY_EQUALS),
    }
}

/// Map Doom-relevant characters (letters/digits/punctuation) to scancodes.
pub fn scancode_from_char(ch: char) -> Option<u8> {
    let upper = ch.to_ascii_uppercase();
    match upper {
        'W' => Some(DOOM_KEY_UP),
        'S' => Some(DOOM_KEY_DOWN),
        'A' => Some(DOOM_KEY_STRAFE_L),
        'D' => Some(DOOM_KEY_STRAFE_R),
        'Q' => Some(DOOM_KEY_STRAFE_L),
        'R' => Some(DOOM_KEY_STRAFE_R),
        'E' => Some(DOOM_KEY_USE),
        'F' => Some(DOOM_KEY_FIRE),
        ' ' => Some(DOOM_KEY_USE),
        '-' => Some(DOOM_KEY_MINUS),
        '=' => Some(DOOM_KEY_EQUALS),
        ',' | '.' | '/' | ';' | '\'' | '[' | ']' | '\\' => Some(upper as u8),
        _ if upper.is_ascii_alphanumeric() => Some(upper as u8),
        _ => None,
    }
}
