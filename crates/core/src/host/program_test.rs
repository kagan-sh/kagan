use std::ffi::OsString;
use std::path::PathBuf;

use super::{ProgramEnvironment, read_environment, user_home};

#[test]
fn a_program_environment_names_its_path_and_shell() {
    let environment = ProgramEnvironment {
        path: "/work/bin".into(),
        shell: "/bin/zsh".into(),
    };
    assert_eq!(
        read_environment(Some(environment)),
        (
            Some(OsString::from("/work/bin")),
            None,
            Some(OsString::from("/bin/zsh"))
        )
    );
}

#[test]
fn the_home_folder_is_the_home_variable() {
    assert_eq!(user_home(), std::env::var_os("HOME").map(PathBuf::from));
}
