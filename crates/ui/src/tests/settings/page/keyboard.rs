use gpui_kit::{AppContext as _, ElementId, TestAppContext, px, size};
use kg_domain::{KeymapOp, Op};

use super::{aria_label, bound_keycap, run_op};
use crate::tests::kagan::{chord, click, label, open, present, shows};
use crate::tests::sidebar::layout::span;

#[gpui_kit::test]
fn leaving_settings_at_480_brings_back_the_window_layout(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(480.), px(720.)));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(
        !shows(cx, window, "settings-page"),
        "escape kept settings open"
    );
    assert!(
        !shows(cx, window, "project-prj_kagan"),
        "the sidebar came back at 480"
    );
    let (_, center) = span(cx, window, "center").unwrap();
    assert!(
        center >= 480. - 0.5,
        "center {center} after settings at 480"
    );
}

#[gpui_kit::test]
fn the_keyboard_page_shows_bound_keycaps_only(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    click(cx, window, "shortcut-NewThread");
    assert!(shows(cx, window, "setting-row-NewThread"));
    assert!(!present(cx, window, "setting-row-NewThread-note"));
    assert_eq!(
        aria_label(cx, window, "setting-row-NewThread").as_deref(),
        Some("New thread")
    );
    assert_eq!(
        aria_label(cx, window, "shortcut-NewThread").as_deref(),
        Some(bound_keycap(cx, "NewThread").as_str())
    );
    for absent in [
        "OpenPalette",
        "ShowHelp",
        "NextFile",
        "PrevFile",
        "RenameThread",
    ] {
        assert!(
            !present(cx, window, &format!("setting-row-{absent}")),
            "{absent} is not bound"
        );
    }
    assert!(!present(cx, window, "settings-heading-shortcuts"));
}

#[gpui_kit::test]
fn keyboard_groups_shortcuts_by_where_they_apply(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    for (section, row) in [
        ("window", "NewThread"),
        ("thread", "StopThread"),
        ("new-thread", "UseProjectFolder"),
        ("review", "NextChange"),
    ] {
        let row = format!("setting-row-{row}");
        click(cx, window, &row);
        cx.update_window(window, |_, window, _| {
            let element =
                crate::ui_registry::find(window, &ElementId::Name(row.clone().into())).unwrap();
            assert!((f32::from(element.bounds.size.height) - 24.).abs() < 0.5);
            assert!(
                element.path.iter().any(|id| *id
                    == ElementId::Name(format!("settings-group-shortcuts-{section}").into()))
            );
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn a_saved_shortcut_changes_the_keycap(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    run_op(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: "NewThread".into(),
            key: "f10".into(),
            reset: false,
        }),
    );
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    click(cx, window, "shortcut-NewThread");
    assert_eq!(
        aria_label(cx, window, "shortcut-NewThread").as_deref(),
        Some(bound_keycap(cx, "NewThread").as_str())
    );
    assert_ne!(
        aria_label(cx, window, "shortcut-NewThread").as_deref(),
        Some("f10")
    );
}

#[gpui_kit::test]
fn a_built_in_shortcut_can_be_removed_and_keeps_no_key(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    click(cx, window, "shortcut-NewThread-remove");
    cx.run_until_parked();
    let binding = cx.update(|cx| {
        kg_core::Workspace::global(cx)
            .read(cx)
            .keymap()
            .binding("NewThread")
            .map(str::to_owned)
    });
    assert_eq!(binding.as_deref(), Some(""));
}

#[gpui_kit::test]
fn a_removed_built_in_shortcut_stays_listed_with_bind_and_reset(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-keyboard");
    click(cx, window, "shortcut-NewThread-remove");
    cx.run_until_parked();
    assert!(
        present(cx, window, "shortcut-NewThread-key"),
        "the removed shortcut vanished from Settings"
    );
    assert_eq!(label(cx, window, "shortcut-NewThread-key"), "Bind");
    click(cx, window, "shortcut-NewThread-reset");
    cx.run_until_parked();
    assert_eq!(label(cx, window, "shortcut-NewThread-key"), "Change");
    let binding = cx.update(|cx| {
        kg_core::Workspace::global(cx)
            .read(cx)
            .keymap()
            .binding("NewThread")
            .map(str::to_owned)
    });
    assert_eq!(binding, None, "reset did not bring the default back");
}
