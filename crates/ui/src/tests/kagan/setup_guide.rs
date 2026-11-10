use gpui_kit::TestAppContext;

use crate::tests::kagan::{click, open, shows};

#[gpui_kit::test]
fn a_fresh_window_shows_the_setup_guide_with_its_open_steps(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(shows(cx, window, "setup-guide"));
    assert!(shows(cx, window, "setup-guide-step-3"));
    assert!(shows(cx, window, "setup-guide-step-4"));
}

#[gpui_kit::test]
fn hide_removes_the_guide_and_settings_brings_it_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "setup-guide-dismiss");
    cx.run_until_parked();
    assert!(!shows(cx, window, "setup-guide"));
    click(cx, window, "settings-gear");
    click(cx, window, "setup-guide-show");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(shows(cx, window, "setup-guide"));
}
