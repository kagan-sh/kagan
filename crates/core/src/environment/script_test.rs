use super::*;

#[test]
fn only_a_full_sha_is_a_commit_id() {
    assert!(commit_id(&"a".repeat(40)));
    assert!(commit_id(&"b".repeat(64)));
    assert!(!commit_id(&"a".repeat(39)));
    assert!(!commit_id(&"a".repeat(41)));
    assert!(!commit_id(&"z".repeat(40)));
}

#[test]
fn a_setup_file_matches_only_the_same_blob_and_mode() {
    let file = Object {
        mode: "100644".into(),
        blob: "one".into(),
    };
    assert!(same(
        &file,
        &Object {
            mode: "100644".into(),
            blob: "one".into()
        }
    ));
    assert!(!same(
        &file,
        &Object {
            mode: "100644".into(),
            blob: "two".into()
        }
    ));
    #[cfg(unix)]
    assert!(!same(
        &file,
        &Object {
            mode: "100755".into(),
            blob: "one".into()
        }
    ));
}

#[test]
fn the_timeout_message_names_the_ten_minute_limit_only_for_it() {
    assert_eq!(
        timeout_message(SETUP, SCRIPT_LIMIT),
        ".kagan/setup ran longer than 10 minutes."
    );
    assert_eq!(
        timeout_message(TEARDOWN, Duration::from_secs(1)),
        ".kagan/teardown timed out."
    );
}

#[test]
fn a_failed_setup_names_its_exit_code_and_stderr() {
    let status = std::process::Command::new("sh")
        .args(["-c", "exit 7"])
        .status()
        .unwrap();
    assert_eq!(
        script_error(SETUP, status, "").to_string(),
        ".kagan/setup failed (exit 7)."
    );
    let status = std::process::Command::new("sh")
        .args(["-c", "echo why >&2; exit 7"])
        .status()
        .unwrap();
    assert_eq!(
        script_error(SETUP, status, "why").to_string(),
        ".kagan/setup failed (exit 7). why"
    );
}

#[test]
fn a_long_line_is_cut_and_marked() {
    let (shown, truncated) = shown_text(&"x".repeat(201));
    assert_eq!(shown, "x".repeat(200));
    assert!(truncated);
}

#[test]
fn shown_text_stops_at_its_character_cap() {
    let lines: Vec<String> = [199, 199, 199, 199, 199, 199, 199, 199, 200, 200]
        .iter()
        .map(|n| "y".repeat(*n))
        .collect();
    let (shown, truncated) = shown_text(&lines.join("\n"));
    assert!(truncated);
    assert_eq!(shown.lines().count(), 9);
    let fits: Vec<String> = [199, 199, 199, 199, 199, 199, 199, 199, 199, 200]
        .iter()
        .map(|n| "y".repeat(*n))
        .collect();
    let (shown, truncated) = shown_text(&fits.join("\n"));
    assert!(!truncated);
    assert_eq!(shown.lines().count(), 10);
}

#[test]
fn a_blank_first_line_keeps_its_newline() {
    assert_eq!(shown_text("\na"), ("\na".to_owned(), false));
}

#[test]
fn a_blank_last_line_is_not_a_cut() {
    assert_eq!(shown_text("a\n\n"), ("a\n".to_owned(), false));
}

#[cfg(unix)]
#[test]
fn file_mode_keeps_the_executable_bit() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("setup");
    std::fs::write(&path, "").unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    assert_eq!(file_mode(&meta), "100644");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    let meta = std::fs::metadata(&path).unwrap();
    assert_eq!(file_mode(&meta), "100755");
}
