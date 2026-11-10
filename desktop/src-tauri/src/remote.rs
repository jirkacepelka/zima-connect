//! Remote control of this computer from another device: touchpad, keys, media keys.

use enigo::{Axis, Button, Coordinate, Direction, Enigo, Key, Keyboard, Mouse, Settings};
use zima_connect_net::RemoteInput;

fn key_for(name: &str) -> Option<Key> {
    Some(match name {
        "enter" => Key::Return,
        "backspace" => Key::Backspace,
        "esc" => Key::Escape,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "left" => Key::LeftArrow,
        "right" => Key::RightArrow,
        "up" => Key::UpArrow,
        "down" => Key::DownArrow,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" => Key::PageUp,
        "pagedown" => Key::PageDown,
        "delete" => Key::Delete,
        _ => return None,
    })
}

fn media_key(action: &str) -> Option<Key> {
    Some(match action {
        "play_pause" => Key::MediaPlayPause,
        "next" => Key::MediaNextTrack,
        "prev" => Key::MediaPrevTrack,
        "vol_up" => Key::VolumeUp,
        "vol_down" => Key::VolumeDown,
        "mute" => Key::VolumeMute,
        _ => return None,
    })
}

/// Carry out one remote command. Errors are ignored on purpose: a missing display or a
/// permission the OS refuses must never take the app down.
pub fn apply(input: &RemoteInput) {
    let Ok(mut e) = Enigo::new(&Settings::default()) else {
        return;
    };
    match input {
        RemoteInput::Move { dx, dy } => {
            let _ = e.move_mouse((*dx).clamp(-2000, 2000), (*dy).clamp(-2000, 2000), Coordinate::Rel);
        }
        RemoteInput::Click { button } => {
            let b = match button.as_str() {
                "right" => Button::Right,
                "middle" => Button::Middle,
                _ => Button::Left,
            };
            let _ = e.button(b, Direction::Click);
            if button == "double" {
                let _ = e.button(Button::Left, Direction::Click);
            }
        }
        RemoteInput::Scroll { dy } => {
            let _ = e.scroll((*dy).clamp(-50, 50), Axis::Vertical);
        }
        RemoteInput::Text { text } => {
            let t: String = text.chars().take(500).collect();
            let _ = e.text(&t);
        }
        RemoteInput::Key { key } => {
            if let Some(k) = key_for(key) {
                let _ = e.key(k, Direction::Click);
            }
        }
        RemoteInput::Media { action } => {
            if let Some(k) = media_key(action) {
                let _ = e.key(k, Direction::Click);
            }
        }
    }
}
