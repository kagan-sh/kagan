use gpui_kit::{Keystroke, Modifiers};
use kg_domain::KeyPlatform;

/// What a key sends to the program on this machine, or `None` when the key is the window's.
pub(crate) fn bytes(keystroke: &Keystroke, app_cursor: bool) -> Option<String> {
    bytes_on(keystroke, app_cursor, KeyPlatform::host())
}

/// What a key sends to the program on `platform`.
pub(crate) fn bytes_on(
    keystroke: &Keystroke,
    app_cursor: bool,
    platform: KeyPlatform,
) -> Option<String> {
    let mods = keystroke.modifiers;
    if mods.platform {
        return None;
    }
    let key = keystroke.key.as_str();
    if is_window_chord(key, mods, platform) {
        return None;
    }
    if let Some(sent) = named(key, mods, app_cursor) {
        return Some(sent);
    }
    if types_text(mods, platform) {
        return keystroke.key_char.clone();
    }
    let text = if mods.control {
        control(key)?.to_string()
    } else if mods.alt {
        // Elsewhere Alt is Meta: the key's own character follows an Escape.
        let mut chars = key.chars();
        let (Some(c), None) = (chars.next(), chars.next()) else {
            return None;
        };
        if mods.shift {
            c.to_ascii_uppercase()
        } else {
            c
        }
        .to_string()
    } else {
        return keystroke
            .key_char
            .clone()
            .or_else(|| (key.chars().count() == 1).then(|| key.to_owned()));
    };
    Some(meta(mods, &text))
}

/// Whether these modifiers type the character the keyboard made: Option on `macOS`, and `AltGr` on
/// Windows, which reports it as Ctrl and Alt together.
fn types_text(mods: Modifiers, platform: KeyPlatform) -> bool {
    match platform {
        KeyPlatform::Mac => mods.alt && !mods.control,
        KeyPlatform::Windows => mods.alt && mods.control,
        KeyPlatform::Linux => false,
    }
}

/// Copy, paste and scroll belong to the window. Off macOS, Ctrl+Shift takes the copy and paste
/// chords, since a bare Ctrl chord goes to the program.
fn is_window_chord(key: &str, mods: Modifiers, platform: KeyPlatform) -> bool {
    let scroll = mods.shift && matches!(key, "pageup" | "pagedown");
    let copy_paste =
        platform != KeyPlatform::Mac && mods.control && mods.shift && matches!(key, "c" | "v");
    scroll || copy_paste
}

fn named(key: &str, mods: Modifiers, app_cursor: bool) -> Option<String> {
    let param = 1 + u8::from(mods.shift) + 2 * u8::from(mods.alt) + 4 * u8::from(mods.control);
    let csi = |final_byte: char| {
        if param > 1 {
            format!("\x1b[1;{param}{final_byte}")
        } else if app_cursor {
            format!("\x1bO{final_byte}")
        } else {
            format!("\x1b[{final_byte}")
        }
    };
    let tilde = |code: u8| {
        if param > 1 {
            format!("\x1b[{code};{param}~")
        } else {
            format!("\x1b[{code}~")
        }
    };
    let function = |final_byte: char| {
        if param > 1 {
            format!("\x1b[1;{param}{final_byte}")
        } else {
            format!("\x1bO{final_byte}")
        }
    };
    Some(match key {
        "up" => csi('A'),
        "down" => csi('B'),
        "right" => csi('C'),
        "left" => csi('D'),
        "home" => csi('H'),
        "end" => csi('F'),
        "insert" => tilde(2),
        "delete" => tilde(3),
        "pageup" => tilde(5),
        "pagedown" => tilde(6),
        "f1" => function('P'),
        "f2" => function('Q'),
        "f3" => function('R'),
        "f4" => function('S'),
        "f5" => tilde(15),
        "f6" => tilde(17),
        "f7" => tilde(18),
        "f8" => tilde(19),
        "f9" => tilde(20),
        "f10" => tilde(21),
        "f11" => tilde(23),
        "f12" => tilde(24),
        "tab" if mods.shift => "\x1b[Z".into(),
        "tab" => meta(mods, "\t"),
        "enter" => meta(mods, "\r"),
        "escape" => meta(mods, "\x1b"),
        "backspace" if mods.control => meta(mods, "\x08"),
        "backspace" => meta(mods, "\x7f"),
        "space" if mods.control => meta(mods, "\0"),
        "space" => meta(mods, " "),
        _ => return None,
    })
}

fn control(key: &str) -> Option<char> {
    let byte = match key {
        "@" | "2" => 0,
        "[" | "3" => 0x1b,
        "\\" | "4" => 0x1c,
        "]" | "5" => 0x1d,
        "^" | "6" => 0x1e,
        "_" | "-" | "7" => 0x1f,
        "8" => 0x7f,
        letter if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() => {
            letter.as_bytes()[0].to_ascii_lowercase() - b'a' + 1
        }
        _ => return None,
    };
    Some(char::from(byte))
}

fn meta(mods: Modifiers, text: &str) -> String {
    if mods.alt {
        format!("\x1b{text}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests;
