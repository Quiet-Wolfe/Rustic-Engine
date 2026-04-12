use rustic_render::health_icon::HealthIcon;
use winit::keyboard::KeyCode;

pub(super) struct FreeplaySong {
    pub name: String,
    pub song_id: String,
    pub character: String,
    pub color: [f32; 3],
    pub week: usize,
    pub icon: Option<HealthIcon>,
}

pub(super) fn srgb_to_linear(s: f32) -> f32 {
    if s <= 0.04045 {
        s / 12.92
    } else {
        ((s + 0.055) / 1.055).powf(2.4)
    }
}

pub(super) fn key_to_char(key: KeyCode) -> Option<char> {
    match key {
        KeyCode::KeyA => Some('a'), KeyCode::KeyB => Some('b'),
        KeyCode::KeyC => Some('c'), KeyCode::KeyD => Some('d'),
        KeyCode::KeyE => Some('e'), KeyCode::KeyF => Some('f'),
        KeyCode::KeyG => Some('g'), KeyCode::KeyH => Some('h'),
        KeyCode::KeyI => Some('i'), KeyCode::KeyJ => Some('j'),
        KeyCode::KeyK => Some('k'), KeyCode::KeyL => Some('l'),
        KeyCode::KeyM => Some('m'), KeyCode::KeyN => Some('n'),
        KeyCode::KeyO => Some('o'), KeyCode::KeyP => Some('p'),
        KeyCode::KeyQ => Some('q'), KeyCode::KeyR => Some('r'),
        KeyCode::KeyS => Some('s'), KeyCode::KeyT => Some('t'),
        KeyCode::KeyU => Some('u'), KeyCode::KeyV => Some('v'),
        KeyCode::KeyW => Some('w'), KeyCode::KeyX => Some('x'),
        KeyCode::KeyY => Some('y'), KeyCode::KeyZ => Some('z'),
        KeyCode::Digit0 => Some('0'), KeyCode::Digit1 => Some('1'),
        KeyCode::Digit2 => Some('2'), KeyCode::Digit3 => Some('3'),
        KeyCode::Digit4 => Some('4'), KeyCode::Digit5 => Some('5'),
        KeyCode::Digit6 => Some('6'), KeyCode::Digit7 => Some('7'),
        KeyCode::Digit8 => Some('8'), KeyCode::Digit9 => Some('9'),
        KeyCode::Space => Some(' '), KeyCode::Minus => Some('-'),
        _ => None,
    }
}

pub(super) fn approx_text_width(text: &str, size: f32) -> f32 {
    text.chars().count() as f32 * size * 0.58
}
