#[cfg(unix)]
use super::fake_program;

#[test]
fn a_program_splits_on_spaces_outside_quotes() {
    let Err(error) = crate::thread::resolve_program("kg-no-such-program trailing", None) else {
        panic!("resolved");
    };
    assert_eq!(
        error.to_string(),
        "program kg-no-such-program was not found"
    );
    let line = if cfg!(windows) {
        "cmd.exe /c echo"
    } else {
        "/bin/sh -c echo"
    };
    let resolved = crate::thread::resolve_program(line, None).unwrap();
    let expected = if cfg!(windows) {
        ["/c", "echo"]
    } else {
        ["-c", "echo"]
    };
    assert_eq!(
        resolved.args,
        expected
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    );
}

#[test]
#[cfg(unix)]
fn quotes_keep_spaces_and_empty_words_and_backslashes_are_plain() {
    let args = |line: &str| {
        crate::thread::resolve_program(line, None)
            .unwrap()
            .args
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(args("/bin/sh -c 'echo a b'"), ["-c", "echo a b"]);
    assert_eq!(args("/bin/sh \"\" x"), ["", "x"]);
    assert_eq!(args("/bin/sh -c a\"b c\"d"), ["-c", "ab cd"]);
    assert_eq!(args("/bin/sh -c 'it\"s'"), ["-c", "it\"s"]);
    assert_eq!(args(r"/bin/sh -c C:\tools\x"), ["-c", r"C:\tools\x"]);
}

#[test]
fn a_program_with_a_bad_quote_or_no_name_is_refused() {
    assert_eq!(
        crate::thread::resolve_program("'/bin/sh -c x", None)
            .err()
            .map(|error| error.to_string()),
        Some("program has an unfinished quote".to_owned())
    );
    assert_eq!(
        crate::thread::resolve_program("\"\" -c x", None)
            .err()
            .map(|error| error.to_string()),
        Some("program is empty".to_owned())
    );
}

#[cfg(unix)]
#[test]
fn the_program_list_finds_known_agents_and_defaults_to_the_first_installed() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "echo 'claude 9.9.9 (test)'");
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        Some(std::ffi::OsStr::new("/bin/sh")),
        &[
            "codex --resume".into(),
            "claude".into(),
            "'unfinished".into(),
        ],
    );
    let claude = list
        .programs
        .iter()
        .find(|p| p.command == "claude")
        .unwrap();
    assert_eq!(claude.source, kg_domain::ProgramSource::Known);
    assert!(claude.installed());
    let codex = list.programs.iter().find(|p| p.command == "codex").unwrap();
    assert!(!codex.installed());
    assert!(
        list.programs
            .iter()
            .any(|p| p.source == kg_domain::ProgramSource::Shell && p.installed())
    );
    let recent = list
        .programs
        .iter()
        .find(|p| p.command == "codex --resume")
        .unwrap();
    assert_eq!(recent.source, kg_domain::ProgramSource::Recent);
    assert!(list.programs.iter().all(|p| p.command != "'unfinished"));
    assert_eq!(list.default.as_deref(), Some("claude"));
}

#[cfg(unix)]
#[test]
fn the_newest_installed_recent_program_is_the_default() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "true");
    fake_program(dir.path(), "tool", "true");
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        None,
        &["tool --fast".into(), "claude".into()],
    );
    assert_eq!(list.default.as_deref(), Some("tool --fast"));
}

#[cfg(unix)]
#[test]
fn only_installed_known_agents_get_a_version() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "echo 'claude 9.9.9 (test)'");
    fake_program(dir.path(), "tool", "echo 'tool 1.0'");
    let mut list =
        crate::host::program::discover(Some(dir.path().as_os_str()), None, None, &["tool".into()]);
    crate::host::program::versions(&mut list);
    let version = |command: &str| {
        list.programs
            .iter()
            .find(|p| p.command == command)
            .and_then(|p| p.version.clone())
    };
    assert_eq!(version("claude").as_deref(), Some("claude 9.9.9 (test)"));
    assert_eq!(version("tool"), None);
    assert_eq!(version("codex"), None);
}
