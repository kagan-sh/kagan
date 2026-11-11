use super::no_window_message;

#[test]
fn a_missing_window_says_how_to_get_one() {
    assert_eq!(no_window_message(0, 0), "no window yet: run kg open");
    assert_eq!(
        no_window_message(1, 1),
        "no window 1; the open windows are numbered 0 to 0"
    );
}
