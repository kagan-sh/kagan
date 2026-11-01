use gpui_kit::Keystroke;

use kg_domain::KeyPlatform;

use super::{bytes, bytes_on};

fn sent(key: &str) -> Option<String> {
    bytes(
        &kg_core::parse_key(key).unwrap().with_simulated_ime(),
        false,
    )
}

#[test]
fn letters_and_symbols_are_their_text() {
    for (key, want) in [("a", "a"), ("shift-a", "A"), ("1", "1"), ("/", "/")] {
        assert_eq!(sent(key).as_deref(), Some(want), "{key}");
    }
}

#[test]
fn the_composed_character_wins_over_the_key() {
    let typed = Keystroke {
        key_char: Some("é".into()),
        ..kg_core::parse_key("e").unwrap()
    };
    assert_eq!(bytes(&typed, false).as_deref(), Some("é"));
}

#[test]
fn a_key_with_no_text_sends_nothing_unless_it_is_one_character() {
    let bare = kg_core::parse_key("q").unwrap();
    assert_eq!(bytes(&bare, false).as_deref(), Some("q"));
    let capslock = kg_core::parse_key("capslock").unwrap();
    assert_eq!(bytes(&capslock, false), None);
}

#[test]
fn control_sends_control_characters() {
    for (key, want) in [
        ("ctrl-a", "\x01"),
        ("ctrl-c", "\x03"),
        ("ctrl-z", "\x1a"),
        ("ctrl-@", "\0"),
        ("ctrl-2", "\0"),
        ("ctrl-[", "\x1b"),
        ("ctrl-3", "\x1b"),
        ("ctrl-\\", "\x1c"),
        ("ctrl-4", "\x1c"),
        ("ctrl-]", "\x1d"),
        ("ctrl-5", "\x1d"),
        ("ctrl-^", "\x1e"),
        ("ctrl-6", "\x1e"),
        ("ctrl-_", "\x1f"),
        ("ctrl--", "\x1f"),
        ("ctrl-7", "\x1f"),
        ("ctrl-8", "\x7f"),
        ("ctrl-space", "\0"),
        ("ctrl-backspace", "\x08"),
    ] {
        assert_eq!(sent(key).as_deref(), Some(want), "{key}");
    }
    assert_eq!(sent("ctrl-1"), None);
    assert_eq!(sent("ctrl-é"), None);
    // Off macOS, Ctrl+Shift+C is the window's copy.
    let shifted = kg_core::parse_key("ctrl-shift-c")
        .unwrap()
        .with_simulated_ime();
    assert_eq!(
        bytes_on(&shifted, false, KeyPlatform::Mac).as_deref(),
        Some("\x03")
    );
}

#[test]
fn alt_is_meta() {
    // Named keys and Ctrl chords send Meta on every platform.
    let mut cases = vec![
        ("alt-ctrl-a", "\x1b\x01"),
        ("alt-enter", "\x1b\r"),
        ("alt-backspace", "\x1b\x7f"),
        ("alt-escape", "\x1b\x1b"),
        ("alt-space", "\x1b "),
        ("alt-tab", "\x1b\t"),
    ];
    // On macOS Option types a character instead, so plain Alt letters are covered by
    // `option_types_the_character` there.
    if KeyPlatform::host() != KeyPlatform::Mac {
        cases.extend([("alt-b", "\x1bb"), ("alt-shift-b", "\x1bB")]);
    }
    for (key, want) in cases {
        assert_eq!(sent(key).as_deref(), Some(want), "{key}");
    }
    assert_eq!(sent("alt-pageup").as_deref(), Some("\x1b[5;3~"));
}

#[test]
fn option_types_the_character() {
    let option_b = Keystroke {
        key_char: Some("∫".into()),
        ..kg_core::parse_key("alt-b").unwrap()
    };
    let option_shift_b = Keystroke {
        key_char: Some("ı".into()),
        ..kg_core::parse_key("alt-shift-b").unwrap()
    };
    assert_eq!(
        bytes_on(&option_b, false, KeyPlatform::Mac).as_deref(),
        Some("∫")
    );
    assert_eq!(
        bytes_on(&option_shift_b, false, KeyPlatform::Mac).as_deref(),
        Some("ı")
    );
    assert_eq!(
        bytes_on(&option_b, false, KeyPlatform::Linux).as_deref(),
        Some("\x1bb")
    );
}

#[test]
fn editing_keys_send_their_codes() {
    for (key, want) in [
        ("enter", "\r"),
        ("tab", "\t"),
        ("shift-tab", "\x1b[Z"),
        ("escape", "\x1b"),
        ("backspace", "\x7f"),
        ("space", " "),
        ("insert", "\x1b[2~"),
        ("delete", "\x1b[3~"),
        ("pageup", "\x1b[5~"),
        ("pagedown", "\x1b[6~"),
    ] {
        assert_eq!(sent(key).as_deref(), Some(want), "{key}");
    }
}

#[test]
fn arrows_follow_the_cursor_mode() {
    for (key, normal, app) in [
        ("up", "\x1b[A", "\x1bOA"),
        ("down", "\x1b[B", "\x1bOB"),
        ("right", "\x1b[C", "\x1bOC"),
        ("left", "\x1b[D", "\x1bOD"),
        ("home", "\x1b[H", "\x1bOH"),
        ("end", "\x1b[F", "\x1bOF"),
    ] {
        let keystroke = kg_core::parse_key(key).unwrap();
        assert_eq!(bytes(&keystroke, false).as_deref(), Some(normal), "{key}");
        assert_eq!(bytes(&keystroke, true).as_deref(), Some(app), "{key}");
    }
}

#[test]
fn modifiers_on_arrows_are_a_parameter() {
    for (key, want) in [
        ("shift-up", "\x1b[1;2A"),
        ("alt-left", "\x1b[1;3D"),
        ("alt-shift-left", "\x1b[1;4D"),
        ("ctrl-right", "\x1b[1;5C"),
        ("ctrl-shift-right", "\x1b[1;6C"),
        ("ctrl-alt-home", "\x1b[1;7H"),
        ("ctrl-alt-shift-end", "\x1b[1;8F"),
        ("ctrl-delete", "\x1b[3;5~"),
    ] {
        let keystroke = kg_core::parse_key(key).unwrap();
        assert_eq!(bytes(&keystroke, true).as_deref(), Some(want), "{key}");
    }
}

#[test]
fn function_keys_send_their_codes() {
    for (key, want) in [
        ("f1", "\x1bOP"),
        ("f2", "\x1bOQ"),
        ("f3", "\x1bOR"),
        ("f4", "\x1bOS"),
        ("f5", "\x1b[15~"),
        ("f6", "\x1b[17~"),
        ("f7", "\x1b[18~"),
        ("f8", "\x1b[19~"),
        ("f9", "\x1b[20~"),
        ("f10", "\x1b[21~"),
        ("f11", "\x1b[23~"),
        ("f12", "\x1b[24~"),
        ("shift-f1", "\x1b[1;2P"),
        ("ctrl-f5", "\x1b[15;5~"),
    ] {
        assert_eq!(sent(key).as_deref(), Some(want), "{key}");
    }
}

#[test]
fn the_command_key_belongs_to_the_window() {
    for key in [
        "cmd-c", "cmd-up", "cmd-q", "cmd-w", "cmd-h", "cmd-m", "cmd-,",
    ] {
        assert_eq!(sent(key), None, "{key}");
    }
}

#[test]
fn the_window_keeps_its_scroll_and_copy_and_paste_chords() {
    assert_eq!(sent("shift-pageup"), None);
    assert_eq!(sent("shift-pagedown"), None);
    let copy = kg_core::parse_key("ctrl-shift-c").unwrap();
    assert_eq!(bytes_on(&copy, false, KeyPlatform::Linux), None);
    let paste = kg_core::parse_key("ctrl-shift-v").unwrap();
    assert_eq!(bytes_on(&paste, false, KeyPlatform::Windows), None);
    assert_eq!(sent("pageup").as_deref(), Some("\x1b[5~"));
}

#[test]
fn option_types_the_character_it_makes_on_macos() {
    let option_b = Keystroke {
        key_char: Some("∫".into()),
        ..kg_core::parse_key("alt-b").unwrap()
    };
    assert_eq!(
        bytes_on(&option_b, false, KeyPlatform::Mac).as_deref(),
        Some("∫")
    );
    // Option as Meta is not a setting, so Linux keeps Alt as Meta.
    assert_eq!(
        bytes_on(&option_b, false, KeyPlatform::Linux).as_deref(),
        Some("\x1bb")
    );
}

#[test]
fn option_with_control_is_a_control_code_on_macos() {
    let option_control_q = Keystroke {
        key_char: Some("@".into()),
        ..kg_core::parse_key("ctrl-alt-q").unwrap()
    };
    assert_eq!(
        bytes_on(&option_control_q, false, KeyPlatform::Mac).as_deref(),
        Some("\x1b\x11")
    );
}

#[test]
fn alt_alone_is_meta_on_windows() {
    let alt_b = Keystroke {
        key_char: Some("∫".into()),
        ..kg_core::parse_key("alt-b").unwrap()
    };
    assert_eq!(
        bytes_on(&alt_b, false, KeyPlatform::Windows).as_deref(),
        Some("\x1bb")
    );
}

#[test]
fn altgr_types_the_character_it_makes_on_windows() {
    let altgr_at = Keystroke {
        key_char: Some("@".into()),
        ..kg_core::parse_key("ctrl-alt-q").unwrap()
    };
    assert_eq!(
        bytes_on(&altgr_at, false, KeyPlatform::Windows).as_deref(),
        Some("@")
    );
    // Ctrl and Alt together without a character is a control code, not text.
    let ctrl_alt = kg_core::parse_key("ctrl-alt-q").unwrap();
    for platform in [KeyPlatform::Linux, KeyPlatform::Windows] {
        assert_eq!(
            bytes_on(&ctrl_alt, false, platform).as_deref(),
            Some("\x1b\x11"),
            "{platform:?}"
        );
    }
}
