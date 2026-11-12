use super::is_known_key;

#[test]
fn a_key_is_one_character_or_a_named_key() {
    for key in ["escape", "enter", "pagedown", "f12", ",", "a", "/"] {
        assert!(is_known_key(key), "{key} should be a key");
    }
    for key in ["notakey", "", "f99", "escap", "ab"] {
        assert!(!is_known_key(key), "{key} should not be a key");
    }
}

#[test]
fn every_named_key_parses_back_to_its_own_name() {
    for name in super::NAMED_KEYS {
        assert!(is_known_key(name), "{name} should be a key");
        assert_eq!(super::parse_key(name).unwrap().key, name);
    }
}

#[test]
fn every_modifier_word_a_person_types_is_a_key_name_gpui_reports() {
    for typed in ["ctrl", "alt", "cmd", "fn", "shift", "secondary"] {
        let key = super::parse_key(typed).unwrap_or_else(|error| panic!("{typed}: {error}"));
        assert!(is_known_key(&key.key), "{typed} parses to {}", key.key);
    }
}
