use std::ffi::OsString;
use std::path::Path;

use crate::app::{data_dir_of, lock_data_dir};

fn os(value: &str) -> OsString {
    OsString::from(value)
}

#[test]
fn the_data_folder_variable_wins_over_home() {
    assert_eq!(
        data_dir_of(Some(os("/custom")), Some(os("/home/me")), None).unwrap(),
        Path::new("/custom")
    );
}

#[test]
fn home_holds_the_default_data_folder() {
    assert_eq!(
        data_dir_of(None, Some(os("/home/me")), None).unwrap(),
        Path::new("/home/me").join(".kagan")
    );
}

#[test]
fn an_empty_data_folder_variable_means_unset() {
    assert_eq!(
        data_dir_of(Some(os("")), Some(os("/home/me")), None).unwrap(),
        Path::new("/home/me").join(".kagan")
    );
}

#[test]
fn an_empty_home_falls_back_to_user_profile() {
    assert_eq!(
        data_dir_of(None, Some(os("")), Some(os("/profile"))).unwrap(),
        Path::new("/profile").join(".kagan")
    );
}

#[test]
fn no_home_or_user_profile_is_an_error() {
    assert!(data_dir_of(None, Some(os("")), None).is_err());
}

#[test]
fn a_data_folder_refuses_a_second_owner_until_the_first_lets_go() {
    let dir = tempfile::tempdir().unwrap();
    let owner = lock_data_dir(dir.path()).unwrap();
    let error = lock_data_dir(dir.path()).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("in use by pid {}", std::process::id())),
        "{error}"
    );
    drop(owner);
    lock_data_dir(dir.path()).unwrap();
}
