use std::ffi::OsString;
use std::path::PathBuf;

use super::{ProgramEnvironment, read_environment, user_home};

#[test]
fn a_program_environment_names_its_path_and_shell() {
    let environment = ProgramEnvironment {
        path: "/work/bin".into(),
        shell: "/bin/zsh".into(),
        home: None,
    };
    assert_eq!(
        read_environment(Some(environment)),
        (
            Some(OsString::from("/work/bin")),
            None,
            Some(OsString::from("/bin/zsh")),
            None
        )
    );
}

#[test]
fn the_home_folder_is_the_home_variable() {
    // SAFETY: nextest runs each test in its own process, so no other thread reads HOME here.
    unsafe { std::env::set_var("HOME", "/kg/test/home") };
    assert_eq!(user_home(), Some(PathBuf::from("/kg/test/home")));
}
