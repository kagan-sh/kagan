use super::directory_and_name;

#[test]
fn a_git_path_splits_on_slash() {
    assert_eq!(directory_and_name("src/main.rs"), ("src", "main.rs"));
    assert_eq!(directory_and_name("a/b/c"), ("a/b", "c"));
    assert_eq!(directory_and_name(r"src\main.rs"), ("", r"src\main.rs"));
    assert_eq!(directory_and_name(r"a/b\c"), ("a", r"b\c"));
    assert_eq!(directory_and_name("main.rs"), ("", "main.rs"));
}
