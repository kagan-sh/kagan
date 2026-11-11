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
