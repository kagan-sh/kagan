use gpui_kit::TestAppContext;
use gpui_kit::component::{ActiveTheme as _, ThemeMode};
use kg_domain::Appearance;

use super::{install_plugins, set_mode};
use crate::tests::kagan::{chord, click, open, open_with_plugins_then, shows};
use crate::theme::follow_system;

#[gpui_kit::test]
fn system_mode_follows_the_operating_system(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    assert!(
        !cx.update(|cx| cx.theme().is_dark()),
        "a light system paints light"
    );

    cx.update(|cx| follow_system(ThemeMode::Dark, cx));
    cx.run_until_parked();
    assert!(
        cx.update(|cx| cx.theme().is_dark()),
        "the window follows a switch to dark"
    );

    cx.update(|cx| follow_system(ThemeMode::Light, cx));
    cx.run_until_parked();
    assert!(!cx.update(|cx| cx.theme().is_dark()), "and back to light");
}

#[gpui_kit::test]
fn a_fixed_mode_ignores_the_operating_system(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    set_mode(cx, Appearance::Dark);
    cx.update(|cx| follow_system(ThemeMode::Light, cx));
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.theme().is_dark()), "dark stays dark");

    set_mode(cx, Appearance::System);
    assert!(
        !cx.update(|cx| cx.theme().is_dark()),
        "system takes the light system back"
    );
}

#[gpui_kit::test]
fn settings_opens_from_its_shortcut_and_escape_goes_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn the_gear_opens_and_closes_the_settings_page(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));
    click(cx, window, "settings-close");
    assert!(!shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn the_gear_leaves_settings_when_clicked_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "settings-page"));
    click(cx, window, "settings-gear");
    assert!(!shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn settings_is_one_page_and_manage_plugins_scrolls_to_installed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_plugins);
    click(cx, window, "settings-gear");
    assert!(shows(cx, window, "setting-row-theme"));
    assert!(!shows(cx, window, "settings-search"));
    assert!(!shows(cx, window, "settings-menu"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    click(cx, window, "plugins-manage");
    assert!(shows(cx, window, "setting-row-install"));
}
