use std::path::{Path, PathBuf};

use gpui_kit::component::WindowExt as _;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Modifiers, TestAppContext};
use kg_core::{Target, Updater, Workspace};
use kg_domain::{Op, OpError, OpResult, UI_PERSON_ONLY, UiOp, UpdateState};
use serde_json::json;

use super::kagan::{click, open, present, press, release, shows, value_of, wait_until};

fn bundle(dir: &Path) -> PathBuf {
    let exe = dir.join("Applications/kagan.app/Contents/MacOS/kg");
    std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
    exe
}

fn offer(cx: &mut TestAppContext, target: Target) {
    cx.update(|cx| {
        Updater::global(cx).update(cx, |updater, cx| {
            updater.use_state(
                UpdateState::Available {
                    version: "99.0.0".into(),
                    notes: "https://github.com/kagan-sh/kagan/releases/tag/v99.0.0".into(),
                },
                target,
                cx,
            );
        });
    });
    cx.run_until_parked();
}

fn ui_click(cx: &mut TestAppContext, name: &str) -> Result<OpResult, OpError> {
    let op = Op::Ui(UiOp::Click {
        element_id: name.into(),
        within: None,
        window: 0,
    });
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task)
}

fn notice(cx: &mut TestAppContext, window: AnyWindowHandle) -> Vec<&'static str> {
    [
        "update-notes",
        "update-later",
        "update-install",
        "update-copy",
        "update-hide",
    ]
    .into_iter()
    .filter(|name| shows(cx, window, name))
    .collect()
}

#[gpui_kit::test]
fn each_install_kind_offers_its_own_way_to_update(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let exe = bundle(data.path());
    let brew = data.path().join("brew");
    std::fs::create_dir_all(brew.join("Caskroom/kagan")).unwrap();
    let image = data.path().join("kagan.AppImage");

    offer(cx, Target::find("macos", &exe, None, None, &[]));
    assert!(shows(cx, window, "update-notice"));
    assert_eq!(
        notice(cx, window),
        [
            "update-notes",
            "update-later",
            "update-install",
            "update-hide"
        ]
    );

    offer(
        cx,
        Target::find(
            "linux",
            Path::new("/opt/.mount_kagan/kg"),
            Some(image),
            Some(Path::new("/opt/.mount_kagan")),
            &[],
        ),
    );
    assert!(notice(cx, window).contains(&"update-install"));

    offer(cx, Target::find("macos", &exe, None, None, &[brew]));
    assert_eq!(
        notice(cx, window),
        ["update-notes", "update-copy", "update-hide"]
    );

    offer(
        cx,
        Target::find("linux", Path::new("/usr/bin/kg"), None, None, &[]),
    );
    assert_eq!(notice(cx, window), ["update-notes", "update-hide"]);

    click(cx, window, "update-hide");
    assert!(!shows(cx, window, "update-notice"));
}

#[gpui_kit::test]
fn kg_ui_cannot_install_an_update(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    offer(
        cx,
        Target::find("macos", &bundle(data.path()), None, None, &[]),
    );
    for name in ["update-install", "update-later"] {
        let clicked = ui_click(cx, name);
        assert!(
            matches!(clicked, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY),
            "{name}: {clicked:?}"
        );
        cx.run_until_parked();
        assert!(matches!(
            cx.update(|cx| Updater::global(cx).read(cx).state().clone()),
            UpdateState::Available { .. }
        ));
    }
    assert!(shows(cx, window, "update-notice"));
}

#[gpui_kit::test]
fn settings_shows_the_version_and_the_automatic_check_starts_off(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    assert!(present(cx, window, "setting-row-update-version"));
    assert!(present(cx, window, "update-check-now"));
    assert_eq!(value_of(cx, window, "update-check").as_deref(), Some("off"));
    click(cx, window, "update-check");
    wait_until(cx, |cx| {
        let task = cx.update(|cx| {
            Workspace::dispatch(
                Op::Preference(kg_domain::PreferenceOp::Get {
                    key: "update-check".into(),
                }),
                cx,
            )
        });
        matches!(cx.foreground_executor().block_test(task), Ok(OpResult::Setting(value)) if value == json!(true))
    });
    assert_eq!(value_of(cx, window, "update-check").as_deref(), Some("on"));
}

#[gpui_kit::test]
fn kg_ui_cannot_confirm_an_install_a_person_asked_for(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    offer(
        cx,
        Target::find("macos", &bundle(data.path()), None, None, &[]),
    );
    let at = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("update-install".into()))
                .map(|element| element.bounds.center())
        })
        .unwrap()
        .unwrap();
    press(cx, window, at, 1, Modifiers::none());
    release(cx, window, at, 1, Modifiers::none());
    cx.run_until_parked();
    let asked = cx
        .update_window(window, |_, window, cx| window.has_active_dialog(cx))
        .unwrap();
    assert!(asked, "a person's click opens the question");

    let op = Op::Ui(UiOp::Key {
        keystroke: "enter".into(),
        window: 0,
    });
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let keyed = cx.foreground_executor().block_test(task);
    cx.run_until_parked();
    assert!(
        matches!(keyed, Err(OpError::Invalid(ref message)) if message == UI_PERSON_ONLY),
        "{keyed:?}"
    );
    assert!(matches!(
        cx.update(|cx| Updater::global(cx).read(cx).state().clone()),
        UpdateState::Available { .. }
    ));
}
