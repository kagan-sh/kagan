use super::is_working;

#[test]
fn the_dot_is_busy_only_while_something_works() {
    assert!(!is_working(0));
    assert!(is_working(1));
    assert!(is_working(2));
}
