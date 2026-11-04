use std::time::Duration;

use gpui_kit::{AnyWindowHandle, AppContext as _, Role, TestAppContext};

use crate::about::{BOOK, LICENSE, NOTICES_ONLINE, REPO, notices, version};
use crate::kagan::menus::About;

use super::kagan::{click, label, open, press_key, role_of, value_of};

fn open_about(cx: &mut TestAppContext, main: AnyWindowHandle) -> AnyWindowHandle {
    cx.dispatch_action(main, About);
    cx.run_until_parked();
    let windows = cx.update(|cx| cx.windows());
    assert_eq!(windows.len(), 2);
    windows.into_iter().find(|window| *window != main).unwrap()
}

fn open_windows(cx: &mut TestAppContext) -> Vec<AnyWindowHandle> {
    let mut windows = cx.update(|cx| cx.windows());
    windows.sort_by_key(AnyWindowHandle::window_id);
    windows
}

#[test]
fn the_version_has_the_commit_only_when_the_build_knew_it() {
    let crate_version = env!("CARGO_PKG_VERSION");
    assert_eq!(
        version(Some("abc1234")),
        format!("{crate_version} (abc1234)")
    );
    assert_eq!(version(None), crate_version);
}

#[gpui_kit::test]
fn about_opens_its_own_window_with_the_name_version_and_footer(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    assert_eq!(role_of(cx, about, "about-icon"), Some(Role::Image));
    assert_eq!(label(cx, about, "about-name"), "kagan");
    assert_eq!(
        label(cx, about, "about-version"),
        format!("Version {}", version(option_env!("KG_COMMIT")))
    );
    assert_eq!(
        label(cx, about, "about-tagline"),
        "A desktop app for coding with agents."
    );
    assert_eq!(
        label(cx, about, "about-footer"),
        "© 2026 Altynbek Orumbayev (MakerX). Apache-2.0 License."
    );
}

#[gpui_kit::test]
fn a_second_about_focuses_the_open_one(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    cx.dispatch_action(main, About);
    cx.run_until_parked();

    assert_eq!(open_windows(cx), [main, about]);
    assert_eq!(cx.update(|cx| cx.active_window()), Some(about));
}

#[gpui_kit::test]
fn clicking_the_version_copies_it_and_says_copied_for_a_moment(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    click(cx, about, "about-version");

    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(
        copied.as_deref(),
        Some(format!("kagan {}", version(option_env!("KG_COMMIT"))).as_str())
    );
    assert_eq!(
        value_of(cx, about, "about-version").as_deref(),
        Some("Copied")
    );

    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(value_of(cx, about, "about-version"), None);
}

#[gpui_kit::test]
fn escape_closes_about_and_leaves_the_main_window(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    cx.simulate_keystrokes(about, "escape");
    cx.run_until_parked();

    assert_eq!(open_windows(cx), [main]);
}

#[gpui_kit::test]
fn close_closes_about_and_leaves_the_main_window(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    cx.simulate_keystrokes(about, "cmd-w");
    cx.run_until_parked();

    assert_eq!(open_windows(cx), [main]);
}

#[gpui_kit::test]
fn a_click_outside_closes_about(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);
    cx.update_window(about, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();

    cx.update_window(main, |_, window, _| window.activate_window())
        .unwrap();
    cx.run_until_parked();

    assert_eq!(open_windows(cx), [main]);
}

#[gpui_kit::test]
fn each_link_opens_its_target(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    for (id, target) in [
        ("about-github", REPO),
        ("about-book", BOOK),
        ("about-license", LICENSE),
        ("about-acknowledgements", NOTICES_ONLINE),
    ] {
        assert_eq!(role_of(cx, about, id), Some(Role::Link));
        assert_eq!(value_of(cx, about, id).as_deref(), Some(target));
        click(cx, about, id);
        assert_eq!(cx.opened_url().as_deref(), Some(target), "{id}");
    }
}

#[test]
fn acknowledgements_open_the_bundled_notices() {
    let bundle = tempfile::tempdir().unwrap();
    let contents = bundle.path().join("kagan.app/Contents");
    std::fs::create_dir_all(contents.join("MacOS")).unwrap();
    std::fs::create_dir_all(contents.join("Resources")).unwrap();
    let exe = contents.join("MacOS/kg");
    assert_eq!(notices(&exe), NOTICES_ONLINE);

    let bundled = contents.join("Resources/THIRD-PARTY-NOTICES.txt");
    std::fs::write(&bundled, "notices").unwrap();
    let url = notices(&exe);
    assert!(url.starts_with("file://"), "{url}");
    assert_eq!(
        url::Url::parse(&url).unwrap().to_file_path().unwrap(),
        bundled
    );
}

#[gpui_kit::test]
fn tab_reaches_the_version_and_each_link_and_enter_uses_it(cx: &mut TestAppContext) {
    let (_kagan, main, _data) = open(cx);
    let about = open_about(cx, main);

    press_key(cx, about, "tab");
    press_key(cx, about, "enter");
    let copied = cx.read_from_clipboard().and_then(|item| item.text());
    assert_eq!(
        copied.as_deref(),
        Some(format!("kagan {}", version(option_env!("KG_COMMIT"))).as_str())
    );

    for target in [REPO, BOOK, LICENSE, NOTICES_ONLINE] {
        press_key(cx, about, "tab");
        press_key(cx, about, "enter");
        assert_eq!(cx.opened_url().as_deref(), Some(target));
    }
}

#[gpui_kit::test]
fn a_start_failure_shows_its_message_with_quit(cx: &mut TestAppContext) {
    let message =
        "/data/kagan.sqlite3 was made by a pre-release build of kagan and cannot be opened.";
    cx.update(|cx| crate::show_start_failure(message.into(), cx).unwrap());
    cx.run_until_parked();
    let windows = open_windows(cx);
    assert_eq!(windows.len(), 1);
    assert_eq!(label(cx, windows[0], "start-failed-message"), message);
    assert_eq!(label(cx, windows[0], "start-failed-quit"), "Quit");
}
