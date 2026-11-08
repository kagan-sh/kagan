use super::*;

#[test]
fn toml_takes_a_windows_path_as_a_literal_string() {
    assert_eq!(
        toml(r"C:\Program Files\kg.exe"),
        r"'C:\Program Files\kg.exe'"
    );
}

#[test]
fn toml_escapes_a_string_that_has_a_single_quote() {
    assert_eq!(toml(r#"it's "x" \ y"#), r#""it's \"x\" \\ y""#);
}

#[test]
fn note_with_appends_plugin_text() {
    let plain = note_with("thr_1", "", "");
    assert!(plain.contains("thr_1"));
    assert!(plain.contains("Task, Agent"));
    assert!(plain.contains("KG_THREAD_ID"));
    assert!(!plain.contains("persona"));
    assert!(!plain.contains("Instructions from your kagan instructions file:"));
    let with = note_with("thr_1", "", "Adopt this persona.");
    assert!(with.starts_with(&plain));
    assert!(with.ends_with("Adopt this persona."));
    let instructed = note_with("thr_1", "Call me Ada.", "");
    assert!(instructed.contains("Instructions from your kagan instructions file:\nCall me Ada."));
    assert!(instructed.contains("thr_1"));
}

#[test]
fn cursor_rule_names_the_variable_and_not_a_thread_id() {
    let rule = cursor_rule("Call me Ada.", "Plugin extra.");
    assert!(rule.starts_with("---\nalwaysApply: true\n---\n"));
    assert!(rule.contains("KG_THREAD_ID"));
    assert!(rule.contains("Task, Agent"));
    assert!(rule.contains("Instructions from your kagan instructions file:\nCall me Ada."));
    assert!(rule.contains("Plugin extra."));
    assert!(!rule.contains("thr_"));
    assert!(!cursor_rule("", "").contains("Instructions from your kagan instructions file:"));
}

#[test]
fn read_instructions_caps_on_a_char_boundary() {
    let dir = tempfile::tempdir().unwrap();
    assert!(read_instructions(dir.path()).is_empty());
    std::fs::write(dir.path().join("instructions.md"), "   \n").unwrap();
    assert!(read_instructions(dir.path()).is_empty());

    let mut ascii = vec![b'a'; 4096];
    ascii.extend(b"TAIL");
    std::fs::write(dir.path().join("instructions.md"), &ascii).unwrap();
    assert_eq!(read_instructions(dir.path()), "a".repeat(4096));

    let mut split = "é".repeat(2047).into_bytes();
    split.extend("€TAIL".as_bytes());
    std::fs::write(dir.path().join("instructions.md"), split).unwrap();
    let text = read_instructions(dir.path());
    assert_eq!(text, "é".repeat(2047));
    assert!(!text.contains('€'));
    assert!(!text.contains("TAIL"));
}
