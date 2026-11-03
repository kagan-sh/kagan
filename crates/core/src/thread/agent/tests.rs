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
    let plain = note("thr_1");
    assert!(plain.contains("thr_1"));
    assert!(!plain.contains("persona"));
    let with = note_with("thr_1", "Adopt this persona.");
    assert!(with.starts_with(&plain));
    assert!(with.ends_with("Adopt this persona."));
}
