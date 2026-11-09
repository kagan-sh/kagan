use super::files::toml;
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

fn line(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

#[test]
fn a_value_flag_hides_the_word_after_it_from_the_subcommand_check() {
    assert!(!subcommand(&Known::Pi, &line(&["--model", "update"])));
    assert!(subcommand(&Known::Pi, &line(&["--model=x", "update"])));
}

#[test]
fn a_flag_that_takes_no_value_leaves_the_next_word_a_subcommand() {
    assert!(subcommand(&Known::Pi, &line(&["--verbose", "update"])));
    assert!(!subcommand(&Known::Pi, &line(&["--verbose", "build"])));
}

#[test]
fn a_word_after_a_double_dash_is_the_subcommand_word() {
    assert!(subcommand(&Known::Kimi, &line(&["--", "doctor"])));
    assert!(subcommand(&Known::Cursor, &line(&["--", "login"])));
    assert!(!subcommand(&Known::Cursor, &line(&["--", "fix the bug"])));
}

#[test]
fn every_agent_kind_has_a_name_in_the_picker_list() {
    for (_, kind) in AGENTS {
        let listed = AGENTS.iter().any(|(name, other)| {
            std::mem::discriminant(other) == std::mem::discriminant(kind)
                && kg_domain::KNOWN_PROGRAMS
                    .iter()
                    .any(|(command, _)| command == name)
        });
        assert!(listed, "an agent kind has no name in KNOWN_PROGRAMS");
    }
    assert!(known(Path::new("agent")).is_some());
}
