use std::ffi::OsStr;
use std::fs;
use std::path::Path;

use super::{find_program, pathext, user_path};

fn temp(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("kg-path-{}-{name}", std::process::id()));
    fs::create_dir_all(&dir).expect("temp dir");
    dir
}

#[test]
fn the_user_path_exists() {
    assert!(user_path().is_some());
}

#[cfg(not(windows))]
#[test]
fn pathext_is_only_set_on_windows() {
    assert_eq!(pathext(), None);
}

#[test]
fn find_program_returns_none_for_a_missing_program() {
    assert_eq!(
        find_program(
            Path::new("kg-definitely-missing"),
            Some(OsStr::new("/nonexistent")),
            None
        ),
        None
    );
}

#[cfg(unix)]
#[test]
fn find_program_finds_an_executable_on_the_path() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp("exe");
    let program = dir.join("kg-test-program");
    fs::write(&program, b"#!/bin/sh\n").expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
    assert_eq!(
        find_program(Path::new("kg-test-program"), Some(dir.as_os_str()), None),
        Some(program)
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn find_program_rejects_a_file_without_the_execute_bit() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp("no-exe");
    let program = dir.join("kg-test-not-executable");
    fs::write(&program, b"data").expect("write");
    fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).expect("chmod");
    assert_eq!(
        find_program(
            Path::new("kg-test-not-executable"),
            Some(dir.as_os_str()),
            None
        ),
        None
    );
    fs::remove_dir_all(&dir).ok();
}

#[cfg(unix)]
#[test]
fn find_program_prefers_the_extension_when_pathext_is_set() {
    use std::os::unix::fs::PermissionsExt as _;
    let dir = temp("pathext");
    for name in ["kg-test-ext", "kg-test-ext.cmd"] {
        let file = dir.join(name);
        fs::write(&file, b"#!/bin/sh\n").expect("write");
        fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).expect("chmod");
    }
    assert_eq!(
        find_program(
            Path::new("kg-test-ext"),
            Some(dir.as_os_str()),
            Some(OsStr::new(".CMD"))
        ),
        Some(dir.join("kg-test-ext.cmd"))
    );
    fs::remove_dir_all(&dir).ok();
}
