use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px};
use kg_core::Workspace;
use kg_domain::{Op, Plugin, PluginId, PluginOp, SettingsOp, SurfaceOp, ThreadId, ThreadOp};
use serde_json::json;

use super::{THROWING_OVERLAY, TINT, TINT_OVERLAY, narrow, run, wide};
use crate::tests::kagan::{
    click, install_overlay, open_with_plugins_then, plugin_log, present, run_shell, shows,
    wait_until,
};

pub(in crate::tests) fn bounds_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .unwrap_or_else(|| panic!("{name} is not drawn"))
            .bounds
    })
    .unwrap()
}

pub(in crate::tests) fn dialog(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    use gpui_kit::component::WindowExt as _;
    cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))
        .unwrap()
}

#[gpui_kit::test]
fn an_overlay_covers_the_center_and_kagan_draws_over_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) =
        open_with_plugins_then(cx, |data| install_overlay(data, TINT, TINT_OVERLAY));
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));
    let center = bounds_of(cx, window, "center");
    assert_eq!(bounds_of(cx, window, "plugin-overlays"), center);
    assert!(center.origin.x > px(0.), "{center:?}");

    let magenta: gpui_kit::Hsla = gpui_kit::rgb(0x00ff_00ff).into();
    let (quads, scaled) = cx
        .update_window(window, |_, window, _| {
            (window.painted_quads(), center.scale(window.scale_factor()))
        })
        .unwrap();
    let tint = quads
        .iter()
        .position(|quad| quad.background == magenta.into())
        .expect("the overlay is painted");
    assert_eq!(quads[tint].bounds, scaled);
    let grid = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("terminal-grid".into()))
                .expect("the terminal grid")
                .bounds
                .scale(window.scale_factor())
        })
        .unwrap();
    let terminal: Vec<_> = quads
        .iter()
        .enumerate()
        .filter(|(_, quad)| quad.bounds.intersects(&grid) && quad.background != magenta.into())
        .map(|(index, _)| index)
        .collect();
    assert!(!terminal.is_empty(), "the terminal paints nothing");
    assert!(
        terminal.iter().all(|index| *index < tint),
        "the terminal is painted over the overlay"
    );

    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    click(cx, window, "quit-cancel");
    cx.run_until_parked();
    assert!(
        !dialog(cx, window),
        "the overlay took the click meant for the dialog"
    );

    let other = run_shell(cx);
    narrow(cx, window);
    click(cx, window, "left-sidebar-toggle");
    wait_until(cx, |cx| shows(cx, window, "left-overlay-panel"));
    click(cx, window, &format!("thread-{}", other.0));
    wait_until(cx, |cx| !shows(cx, window, "left-overlay-panel"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(other));
    assert!(shows(cx, window, &overlay));
}

#[gpui_kit::test]
fn disable_reload_and_removal_take_the_overlay_away(cx: &mut TestAppContext) {
    let (_kagan, window, data) =
        open_with_plugins_then(cx, |data| install_overlay(data, TINT, TINT_OVERLAY));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));
    let plugin = PluginId(TINT.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: plugin.clone() })).unwrap();
    cx.run_until_parked();
    assert!(!present(cx, window, &overlay));
    run(cx, Op::Plugin(PluginOp::Enable { id: plugin.clone() })).unwrap();
    wait_until(cx, |cx| shows(cx, window, &overlay));

    let log = data.path().join("plugins").join(TINT).join("logs.jsonl");
    let mounts = || std::fs::read_to_string(&log).map_or(0, |text| text.matches("mounted").count());
    let before = mounts();
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: plugin.clone(),
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| mounts() > before && shows(cx, window, &overlay));

    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: plugin,
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval")
            .id
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!present(cx, window, &overlay));
    assert!(!present(cx, window, "plugin-overlays"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_overlay_that_throws_draws_nothing_and_the_terminal_still_types(cx: &mut TestAppContext) {
    let plugin = "dev.test.throwing";
    let (kagan, window, _data) =
        open_with_plugins_then(cx, |data| install_overlay(data, plugin, THROWING_OVERLAY));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{plugin}");
    wait_until(cx, |cx| !present(cx, window, &overlay));
    cx.simulate_input(window, "echo still''here");
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| {
        crate::tests::kagan::grid_text(cx, &kagan).contains("stillhere")
    });
    assert!(!present(cx, window, &overlay));
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("boom in the overlay")
    });
}

pub(in crate::tests) fn install_example_overlay(data: &Path) {
    let root = dunce::canonicalize(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins/overlay"),
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId("dev.kagan.overlay".into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

/// The example's bubble: a 96 point square box, painted.
pub(in crate::tests) fn bubbles(cx: &mut TestAppContext, window: AnyWindowHandle) -> usize {
    cx.update_window(window, |_, window, _| {
        let side = 96. * window.scale_factor();
        window
            .painted_quads()
            .iter()
            .filter(|quad| {
                (quad.bounds.size.width.0 - side).abs() < 1.
                    && (quad.bounds.size.height.0 - side).abs() < 1.
            })
            .count()
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_example_overlay_draws_and_its_setting_hides_it(cx: &mut TestAppContext) {
    let plugin = "dev.kagan.overlay";
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_example_overlay);
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| bubbles(cx, window) == 1);
    assert!(shows(cx, window, &format!("plugin-overlay-{plugin}")));
    run(
        cx,
        Op::Plugin(PluginOp::Config {
            id: PluginId(plugin.into()),
            op: kg_domain::ConfigOp::Set {
                key: "shown".into(),
                value: Some(json!(false)),
            },
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| bubbles(cx, window) == 0);
    let log = plugin_log(cx, plugin);
    assert!(
        !log.contains("crashed") && !log.contains("paint-only"),
        "{log}"
    );
}

pub(in crate::tests) fn ask(cx: &mut TestAppContext, id: &ThreadId) {
    let asked = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: id.clone(),
                title: "Keep going?".into(),
                options: ["yes", "no"]
                    .map(|option| kg_domain::InteractionOption {
                        id: option.into(),
                        label: option.into(),
                        role: None,
                    })
                    .into(),
                fields: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(asked).unwrap();
}

#[gpui_kit::test]
fn an_overlay_hides_while_a_question_waits_and_never_draws_over_settings(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, TINT, TINT_OVERLAY);
        install_overlay(data, "dev.test.other", TINT_OVERLAY);
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));

    run(cx, Op::Settings(SettingsOp::Plugins)).unwrap();
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
    assert!(!present(cx, window, "plugin-overlays"));
    run(cx, Op::Surface(SurfaceOp::Threads)).unwrap();
    wait_until(cx, |cx| shows(cx, window, &overlay));

    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: PluginId("dev.test.other".into()),
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    wait_until(cx, |cx| !present(cx, window, "plugin-overlays"));
    run(cx, Op::Settings(SettingsOp::Plugins)).unwrap();
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
    assert!(!present(cx, window, "plugin-overlays"));
    drop(remove);
}

#[gpui_kit::test]
fn an_overlay_hides_while_its_thread_asks(cx: &mut TestAppContext) {
    let (_kagan, window, _data) =
        open_with_plugins_then(cx, |data| install_overlay(data, TINT, TINT_OVERLAY));
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let overlay = format!("plugin-overlay-{TINT}");
    wait_until(cx, |cx| shows(cx, window, &overlay));
    ask(cx, &id);
    wait_until(cx, |cx| !present(cx, window, &overlay));
}

pub(in crate::tests) fn image_refused(cx: &mut TestAppContext, plugin: &str, source: &str) {
    let attack = format!(".child(image({source:?}))");
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &crate::tests::kagan::overlay_source(&attack));
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        plugin_log(cx, plugin).contains("relative path inside the application's folder")
    });
    wait_until(cx, |cx| {
        !present(cx, window, &format!("plugin-overlay-{plugin}"))
    });
}

#[gpui_kit::test]
fn an_overlay_image_from_a_url_is_refused(cx: &mut TestAppContext) {
    image_refused(cx, "dev.test.url", "https://example.com/a.png?d=secret");
}

#[gpui_kit::test]
fn an_overlay_image_that_leaves_the_folder_is_refused(cx: &mut TestAppContext) {
    image_refused(cx, "dev.test.outside", "../../secret.png");
}

#[gpui_kit::test]
fn an_overlay_that_opens_a_popover_is_refused_and_dialogs_take_clicks(cx: &mut TestAppContext) {
    let plugin = "dev.test.popover";
    let attack = ".child(Popover.new(\"menu\").open(true).trigger(div()).content(div().w(600).h(600).child(\"Deny\")))";
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_overlay(data, plugin, &crate::tests::kagan::overlay_source(attack));
    });
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| plugin_log(cx, plugin).contains("Popover"));
    assert!(!present(cx, window, &format!("plugin-overlay-{plugin}")));
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    click(cx, window, "quit-cancel");
    cx.run_until_parked();
    assert!(!dialog(cx, window));
}
