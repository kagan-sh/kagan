use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, FocusHandle, TestAppContext, px, size,
};
use kg_core::{Target, Updater, Workspace};
use kg_domain::{
    InteractionOption, Op, OpResult, Plugin, PluginId, PluginOp, ThreadId, ThreadOp, UpdateState,
    WindowOp,
};

use crate::kagan::screen_read;
use crate::tests::kagan::{
    click, focused_of, label, open, open_with_plugins_then, role_of, run_shell, selected_of, shown,
    shows, thread_record, value_of, wait_until,
};

#[cfg(unix)]
use crate::tests::kagan::run_program;

fn bounds_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> (f32, f32, f32, f32) {
    cx.update_window(window, |_, window, _| {
        let Some(bounds) =
            crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
                .map(|element| element.bounds)
        else {
            return (0., 0., 0., 0.);
        };
        (
            bounds.origin.x.as_f32(),
            bounds.origin.y.as_f32(),
            bounds.size.width.as_f32(),
            bounds.size.height.as_f32(),
        )
    })
    .unwrap()
}

fn height_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> f32 {
    bounds_of(cx, window, name).3
}

fn mark_read(cx: &mut TestAppContext, id: &ThreadId) {
    let task =
        cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

fn raise(cx: &mut TestAppContext, id: &ThreadId, title: &str) -> gpui_kit::Task<TestAnswer> {
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    }];
    let asked = cx.update(|cx| Workspace::ask(id, title.into(), options, cx));
    cx.run_until_parked();
    asked
}

type TestAnswer = std::result::Result<String, kg_domain::OpError>;

const STATUS: &str = "dev.test.status";
const STATUS_ITEM: &str = "activity-plugin-dev.test.status";

fn install_status(data: &std::path::Path) {
    install_status_plugin(data, STATUS, "Usage", "42%");
}

fn install_status_plugin(data: &std::path::Path, id: &str, title: &str, text: &str) {
    install_status_plugin_with(data, id, title, None, text);
}

// `command` is one of the plugin's own commands, run when the strip line is clicked.
fn install_status_plugin_with(
    data: &std::path::Path,
    id: &str,
    title: &str,
    command: Option<&str>,
    text: &str,
) {
    let root = data.join(id.rsplit('.').next().unwrap_or(id));
    std::fs::create_dir(&root).unwrap();
    let mut status = serde_json::json!({ "title": title });
    let mut commands = serde_json::json!([]);
    if let Some(command) = command {
        status["command"] = command.into();
        commands = serde_json::json!([{ "id": command, "title": "Refresh usage" }]);
    }
    let manifest = serde_json::json!({
        "id": id,
        "name": "Status",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Show a line in the activity strip.",
        "powers": {},
        "commands": commands,
        "ui": { "status": status },
        "tools": [{
            "name": "status",
            "description": "Change the status line",
            "input": "schemas/any.json",
            "scope": "all"
        }]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        format!(
            r#"import {{ View }} from "gpui-kit";
import {{ next, reply }} from "kagan/jobs";
import {{ setStatus }} from "kagan/host";
export default class Plugin extends View {{
  init(_props, cx) {{
    setStatus({text}, {{ tone: "busy" }});
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        if (job.kind === "tool" && job.name === "status") {{
          setStatus(job.input.text);
          reply(job.id, {{ text: "ok" }});
        }} else {{
          reply(job.id, {{ text: "ok" }});
        }}
      }}
    }});
  }}
}}
"#,
            text = serde_json::to_string(text).unwrap(),
        ),
    )
    .unwrap();
    std::fs::create_dir(root.join("schemas")).unwrap();
    std::fs::write(root.join("schemas/any.json"), r#"{"type":"object"}"#).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({
                "powers": {},
                "folders": {},
                "status": title,
                "status_command": command.map(|id| serde_json::json!({ "id": id, "title": "Refresh usage" })),
            }),
            error: None,
        })
        .unwrap();
}

const TIMER: &str = "dev.test.push";
const TIMER_ITEM: &str = "activity-plugin-dev.test.push";

fn install_timer_status(data: &std::path::Path) {
    let root = data.join("timer");
    std::fs::create_dir(&root).unwrap();
    let manifest = serde_json::json!({
        "id": TIMER,
        "name": "Push",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Push a line in the activity strip.",
        "powers": {},
        "ui": { "status": { "title": "Push" } }
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View } from "gpui-kit";
import { setStatus } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    setStatus("42%", { tone: "busy" });
    cx.timer.after(100, () => setStatus("77%"));
  }
}
"#,
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(TIMER.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({"powers": {}, "folders": {}, "status": "Push"}),
            error: None,
        })
        .unwrap();
}

#[cfg(unix)]
fn busy_program() -> String {
    "/bin/sh -c \"printf '\\033]0;\\342\\240\\213 working\\007'; sleep 30\"".into()
}

fn offer_update(cx: &mut TestAppContext, dir: &std::path::Path) {
    cx.update(|cx| {
        Updater::global(cx).update(cx, |updater, cx| {
            updater.use_state(
                UpdateState::Available {
                    version: "99.0.0".into(),
                    notes: String::new(),
                },
                Target::find("macos", &dir.join("kagan"), None, None, &[]),
                cx,
            );
        });
    });
    cx.run_until_parked();
}

fn terminal_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_focused(kagan.read(cx), window, cx)
    })
    .unwrap()
}

#[gpui_kit::test]
fn the_strip_starts_closed_and_quiet_at_the_bottom(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert!(shows(cx, window, "activity-strip"));
    assert_eq!(
        value_of(cx, window, "activity-summary").as_deref(),
        Some("Quiet")
    );
    assert_eq!(selected_of(cx, window, "activity-toggle"), Some(false));
    assert!(!shows(cx, window, "activity-log"));
    assert!(!shows(cx, window, "activity-needs"));
    assert!(!shows(cx, window, "activity-update"));
    let (x, y, width, height) = bounds_of(cx, window, "activity-strip");
    assert!((height - 28.).abs() < 0.5, "the closed strip is {height}");
    let (viewport_width, viewport_height) = cx
        .update_window(window, |_, window, _| {
            let size = window.viewport_size();
            (f32::from(size.width), f32::from(size.height))
        })
        .unwrap();
    assert!(x.abs() < 0.5, "the strip starts at {x}");
    assert!(
        (width - viewport_width).abs() < 0.5,
        "the strip is {width} of {viewport_width}"
    );
    assert!(
        (y + height - viewport_height).abs() < 0.5,
        "the strip ends at {} of {viewport_height}",
        y + height
    );
}

#[gpui_kit::test]
fn the_open_strip_sits_under_the_center_and_inside_the_window(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "activity-toggle");
    assert_open_strip_clears_center(cx, window);
}

#[gpui_kit::test]
fn the_open_strip_clears_the_center_at_the_minimum_height(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1120.), px(360.)));
    cx.run_until_parked();
    click(cx, window, "activity-toggle");
    assert_open_strip_clears_center(cx, window);
}

fn assert_open_strip_clears_center(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let (_, center_y, _, center_height) = bounds_of(cx, window, "center");
    let (_, strip_y, _, strip_height) = bounds_of(cx, window, "activity-strip");
    let viewport_height = cx
        .update_window(window, |_, window, _| {
            f32::from(window.viewport_size().height)
        })
        .unwrap();
    assert!(
        center_y + center_height <= strip_y + 0.5,
        "the center ends at {} but the strip starts at {strip_y}",
        center_y + center_height
    );
    assert!(
        (strip_y + strip_height - viewport_height).abs() < 0.5,
        "the open strip ends at {} of {viewport_height}",
        strip_y + strip_height
    );
}

#[gpui_kit::test]
fn the_summary_text_is_status_and_the_log_button_is_the_only_toggle(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    assert_eq!(
        role_of(cx, window, "activity-summary"),
        Some(gpui_kit::Role::Status)
    );
    assert_eq!(
        role_of(cx, window, "activity-toggle"),
        Some(gpui_kit::Role::Button)
    );
    assert!(!shows(cx, window, "activity-log"));
}

#[gpui_kit::test]
fn the_open_log_says_nothing_has_happened_yet(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "activity-toggle");
    assert!(shows(cx, window, "activity-empty"));
    assert_eq!(
        label(cx, window, "activity-empty"),
        "Nothing has happened yet."
    );
    assert!(!shows(cx, window, "activity-row-0"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_busy_agent_shows_in_the_summary_and_as_a_log_line(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let id = run_program(cx, &busy_program());
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        value_of(cx, window, "activity-summary").as_deref() == Some("1 agent working")
    });
    assert!(
        terminal_focused(cx, window, &kagan),
        "a turn moved the keyboard"
    );
    assert_ne!(focused_of(cx, window, "activity-strip"), Some(true));

    assert!(!shows(cx, window, "activity-update"));
    offer_update(cx, data.path());
    assert!(shows(cx, window, "activity-update"));
    assert!(
        terminal_focused(cx, window, &kagan),
        "an update moved the keyboard"
    );
    click(cx, window, "update-hide");
    assert!(
        !shows(cx, window, "activity-update"),
        "Not now silenced the center notice but not the strip"
    );

    click(cx, window, "activity-toggle");
    let title = thread_record(cx, &id).title;
    assert_eq!(
        label(cx, window, "activity-row-0"),
        format!("{title}: turn started")
    );
}

#[gpui_kit::test]
fn a_question_on_the_shown_thread_is_not_counted(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    mark_read(cx, &id);
    click(cx, window, &format!("thread-{}", id.0));

    let asked = raise(cx, &id, "Shown?");
    assert!(
        !shows(cx, window, "activity-needs"),
        "the strip counted the thread on screen, which NextThreadNeedingYou skips"
    );
    drop(asked);
}

#[gpui_kit::test]
fn a_waiting_question_shows_and_its_click_opens_the_next_thread_needing_you(
    cx: &mut TestAppContext,
) {
    let (kagan, window, _data) = open(cx);
    let oldest = run_shell(cx);
    let newest = run_shell(cx);
    let current = run_shell(cx);
    for id in [&oldest, &newest, &current] {
        mark_read(cx, id);
    }
    click(cx, window, &format!("thread-{}", current.0));
    let asked = raise(cx, &oldest, "First?");
    assert_eq!(value_of(cx, window, "activity-needs").as_deref(), Some("1"));
    click(cx, window, "activity-needs");
    assert_eq!(shown(cx, &kagan), Some(oldest));
    assert!(
        !shows(cx, window, "activity-log"),
        "the needs-you click only navigates"
    );
    drop(asked);
}

#[gpui_kit::test]
fn the_log_button_opens_the_log_and_escape_closes_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));

    click(cx, window, "activity-toggle");
    assert!(shows(cx, window, "activity-log"));
    assert_eq!(selected_of(cx, window, "activity-toggle"), Some(true));
    assert_ne!(
        focused_of(cx, window, "activity-strip"),
        Some(true),
        "the strip never takes the keyboard"
    );
    let height = height_of(cx, window, "activity-strip");
    assert!((height - 168.).abs() < 0.5, "the open strip is {height}");

    // A person's click focuses the toggle; the test click does not, so focus it the same way.
    let focus = cx
        .update(|cx| kagan.read(cx).activity_focus("toggle"))
        .expect("the toggle has a focus handle");
    cx.update_window(window, |_, window, cx| window.focus(&focus, cx))
        .unwrap();
    crate::tests::kagan::press_key(cx, window, "escape");
    assert!(!shows(cx, window, "activity-log"));
    assert_eq!(selected_of(cx, window, "activity-toggle"), Some(false));
}

// Starts an install of a new plugin folder. The install waits for the person's OK.
fn start_install(
    cx: &mut TestAppContext,
    folder: &std::path::Path,
) -> gpui_kit::Task<Result<OpResult, kg_domain::OpError>> {
    let created = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::New {
                name: "widget".into(),
                dir: Some(folder.to_path_buf()),
            }),
            cx,
        )
    });
    assert!(matches!(
        cx.foreground_executor().block_test(created),
        Ok(OpResult::Files(_))
    ));
    let source = folder.display().to_string();
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source,
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    })
}

fn assert_log_toggle_inside(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let (x, _, width, _) = bounds_of(cx, window, "activity-toggle");
    let viewport_width = cx
        .update_window(window, |_, window, _| {
            f32::from(window.viewport_size().width)
        })
        .unwrap();
    assert!(
        x >= 0. && x + width <= viewport_width + 0.5,
        "the log toggle spans {x}..{} of {viewport_width}",
        x + width
    );
}

#[gpui_kit::test]
fn a_plugin_install_shows_its_source_and_a_log_line(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, |_| {});
    cx.run_until_parked();
    let folder = data.path().join("widget");
    let install = start_install(cx, &folder);
    let source = folder.display().to_string();
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    assert!(shows(cx, window, "activity-install"));

    click(cx, window, "activity-toggle");
    assert_eq!(
        label(cx, window, "activity-row-0"),
        format!("Installing {source}")
    );
    click(cx, window, "activity-install");
    assert!(
        cx.update(|cx| screen_read::in_settings(kagan.read(cx))),
        "the Installing button opens Settings"
    );

    let approval = cx
        .update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .approvals()
                .next()
                .map(|approval| approval.id.clone())
        })
        .expect("the install question waits");
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(install).unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "activity-install"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_open_strip_keeps_its_log_across_a_thread_switch(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let first = run_program(cx, &busy_program());
    click(cx, window, &format!("thread-{}", first.0));
    click(cx, window, "activity-toggle");
    wait_until(cx, |cx| shows(cx, window, "activity-row-0"));

    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", second.0));
    assert!(shows(cx, window, "activity-strip"));
    assert!(shows(cx, window, "activity-log"));
    assert!(shows(cx, window, "activity-row-0"));
}

#[gpui_kit::test]
fn a_plugin_status_item_is_one_text_line_in_the_strip(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_status);
    wait_until(cx, |cx| shows(cx, window, STATUS_ITEM));
    assert_eq!(
        role_of(cx, window, STATUS_ITEM),
        Some(gpui_kit::Role::Status)
    );
    assert_eq!(label(cx, window, STATUS_ITEM), "Usage");
    assert_eq!(value_of(cx, window, STATUS_ITEM).as_deref(), Some("42%"));
    assert_eq!(focused_of(cx, window, STATUS_ITEM), None);
    assert_ne!(focused_of(cx, window, "activity-strip"), Some(true));
}

#[gpui_kit::test]
fn the_plugin_status_preference_hides_and_shows_every_item(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_status);
    wait_until(cx, |cx| shows(cx, window, STATUS_ITEM));

    click(cx, window, "settings-gear");
    click(cx, window, "plugin-status");
    wait_until(cx, |cx| {
        crate::tests::settings::page::saved(cx, "plugin-status") == serde_json::json!(false)
    });
    assert!(!shows(cx, window, STATUS_ITEM));

    click(cx, window, "plugin-status");
    wait_until(cx, |cx| {
        crate::tests::settings::page::saved(cx, "plugin-status") == serde_json::json!(true)
    });
    assert!(shows(cx, window, STATUS_ITEM));
}

#[gpui_kit::test]
fn a_narrow_window_keeps_the_host_summary_and_hides_plugin_items(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_status);
    wait_until(cx, |cx| shows(cx, window, STATUS_ITEM));

    cx.simulate_window_resize(window, size(px(700.), px(720.)));
    assert!(!shows(cx, window, STATUS_ITEM));
    assert!(shows(cx, window, "activity-summary"));

    cx.simulate_window_resize(window, size(px(1000.), px(720.)));
    assert!(shows(cx, window, STATUS_ITEM));
}

#[gpui_kit::test]
fn disabling_the_plugin_removes_its_item(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_status);
    wait_until(cx, |cx| shows(cx, window, STATUS_ITEM));

    let disabled = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Disable {
                id: PluginId(STATUS.into()),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(disabled).unwrap();
    wait_until(cx, |cx| !shows(cx, window, STATUS_ITEM));
}

#[gpui_kit::test]
fn a_timer_push_redraws_the_strip_without_a_workspace_event(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_timer_status);
    wait_until(cx, |cx| shows(cx, window, TIMER_ITEM));
    assert_eq!(value_of(cx, window, TIMER_ITEM).as_deref(), Some("42%"));
    wait_until(cx, |cx| {
        value_of(cx, window, TIMER_ITEM).as_deref() == Some("77%")
    });
}

#[gpui_kit::test]
fn four_long_items_fit_beside_the_summary_at_720(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        for number in 1..=4 {
            install_status_plugin(
                data,
                &format!("dev.test.status{number}"),
                &format!("Usage {number}"),
                &"x".repeat(120),
            );
        }
    });
    cx.simulate_window_resize(window, size(px(720.), px(720.)));
    let names: Vec<String> = (1..=4)
        .map(|number| format!("activity-plugin-dev.test.status{number}"))
        .collect();
    wait_until(cx, |cx| names.iter().all(|name| shows(cx, window, name)));

    let (strip_x, _, strip_width, _) = bounds_of(cx, window, "activity-strip");
    let (_, _, summary_width, summary_height) = bounds_of(cx, window, "activity-summary");
    assert!(
        summary_width > 0. && summary_height > 0.,
        "the summary collapsed: {summary_width} by {summary_height}"
    );
    for name in &names {
        let (x, _, width, height) = bounds_of(cx, window, name);
        assert!(width > 0. && height > 0., "{name} has no bounds");
        assert!(
            x + width <= strip_x + strip_width + 0.5,
            "{name} ends at {} past the strip's right edge {}",
            x + width,
            strip_x + strip_width
        );
    }
}

#[gpui_kit::test]
fn the_strip_draws_four_of_five_items(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        for number in 1..=5 {
            install_status_plugin(
                data,
                &format!("dev.test.status{number}"),
                &format!("Usage {number}"),
                "42%",
            );
        }
    });
    cx.simulate_window_resize(window, size(px(1400.), px(720.)));
    let names: Vec<String> = (1..=5)
        .map(|number| format!("activity-plugin-dev.test.status{number}"))
        .collect();
    wait_until(cx, |cx| {
        names[..4].iter().all(|name| shows(cx, window, name))
    });
    assert!(!shows(cx, window, &names[4]), "the fifth item was drawn");
    let kept = cx.read(|cx| {
        kg_plugins::PluginStrip::global(cx)
            .map(|strip| strip.read(cx).items().count())
            .unwrap_or_default()
    });
    assert_eq!(kept, 5, "the host dropped an item instead of the strip");
}

#[gpui_kit::test]
fn a_question_does_not_take_the_keyboard_or_open_the_strip(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let asking = run_shell(cx);
    let shown_thread = run_shell(cx);
    for id in [&asking, &shown_thread] {
        mark_read(cx, id);
    }
    click(cx, window, &format!("thread-{}", shown_thread.0));
    assert!(terminal_focused(cx, window, &kagan));

    let asked = raise(cx, &asking, "Second?");
    assert!(
        terminal_focused(cx, window, &kagan),
        "the question moved the keyboard"
    );
    assert_ne!(focused_of(cx, window, "activity-strip"), Some(true));
    assert!(!shows(cx, window, "activity-log"));
    drop(asked);
}

#[gpui_kit::test]
fn the_update_button_is_a_button_and_does_not_open_the_log(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    offer_update(cx, data.path());
    assert_eq!(
        role_of(cx, window, "activity-update"),
        Some(gpui_kit::Role::Button)
    );
    click(cx, window, "activity-update");
    assert!(!shows(cx, window, "activity-log"));
}

#[gpui_kit::test]
fn clicking_a_status_line_with_a_command_runs_that_command(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_status_plugin_with(data, STATUS, "Usage", Some("usage.refresh"), "42%");
    });
    wait_until(cx, |cx| shows(cx, window, STATUS_ITEM));
    assert_eq!(
        role_of(cx, window, STATUS_ITEM),
        Some(gpui_kit::Role::Button)
    );
    assert_eq!(label(cx, window, STATUS_ITEM), "Usage");
    assert_eq!(value_of(cx, window, STATUS_ITEM).as_deref(), Some("42%"));
    let hint = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(STATUS_ITEM.to_owned().into()))
                .and_then(|element| element.hint.map(|hint| hint.to_string()))
        })
        .unwrap()
        .unwrap_or_default();
    assert!(
        hint.contains("Refresh usage"),
        "the button says what it runs: {hint:?}"
    );

    click(cx, window, STATUS_ITEM);
    click(cx, window, "activity-toggle");
    assert_eq!(
        label(cx, window, "activity-row-0"),
        format!("{STATUS}: ran a command")
    );
}

#[gpui_kit::test]
fn each_strip_control_is_one_tab_stop_in_tab_order(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, |data| {
        install_status_plugin_with(data, STATUS, "Usage", Some("usage.refresh"), "42%");
    });
    wait_until(cx, |cx| shows(cx, window, STATUS_ITEM));
    offer_update(cx, data.path());
    let waiting = run_shell(cx);
    let _asked = raise(cx, &waiting, "First?");
    assert!(shows(cx, window, "activity-needs"));
    assert!(shows(cx, window, "activity-update"));

    let names = ["needs", "update", "plugin-dev.test.status", "toggle"];
    let handles: Vec<(&str, FocusHandle)> = names
        .iter()
        .map(|name| {
            let focus = cx.update(|cx| kagan.read(cx).activity_focus(name));
            (
                *name,
                focus.unwrap_or_else(|| panic!("{name} has no focus handle")),
            )
        })
        .collect();

    let mut seen: Vec<&str> = Vec::new();
    for _ in 0..600 {
        crate::tests::kagan::press_key(cx, window, "tab");
        let now = cx
            .update_window(window, |_, window, _| {
                handles
                    .iter()
                    .find(|(_, focus)| focus.is_focused(window))
                    .map(|(name, _)| *name)
            })
            .unwrap();
        seen.push(now.unwrap_or("-"));
    }
    let start = seen
        .iter()
        .position(|name| *name == "needs")
        .expect("Tab reaches the needs-you button");
    let end = start
        + 1
        + seen[start + 1..]
            .iter()
            .position(|name| *name == "needs")
            .expect("Tab comes back to the needs-you button");
    let cycle = &seen[start..end];
    for name in names {
        let count = cycle.iter().filter(|seen| **seen == name).count();
        assert_eq!(count, 1, "{name} is {count} Tab stops in {cycle:?}");
    }
    let order: Vec<usize> = names
        .iter()
        .map(|name| cycle.iter().position(|seen| seen == name).unwrap())
        .collect();
    assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{cycle:?}");

    let toggle = handles[3].1.clone();
    cx.update_window(window, |_, window, cx| window.focus(&toggle, cx))
        .unwrap();
    crate::tests::kagan::press_key(cx, window, "enter");
    assert!(shows(cx, window, "activity-log"), "Enter opens the log");
}

#[gpui_kit::test]
fn the_activity_log_op_shows_and_hides_the_log(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let toggle = |cx: &mut TestAppContext| {
        let task = cx.update(|cx| Workspace::dispatch(Op::Window(WindowOp::ToggleActivityLog), cx));
        cx.foreground_executor().block_test(task).unwrap();
        cx.run_until_parked();
    };
    toggle(cx);
    assert!(shows(cx, window, "activity-log"));
    assert_eq!(selected_of(cx, window, "activity-toggle"), Some(true));
    toggle(cx);
    assert!(!shows(cx, window, "activity-log"));
}

#[gpui_kit::test]
fn long_plugin_command_items_keep_the_log_toggle_inside_the_window(cx: &mut TestAppContext) {
    let long = "word ".repeat(24);
    let long = long.trim_end();
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        for n in 1..=4 {
            let command = format!("usage.refresh{n}");
            install_status_plugin_with(
                data,
                &format!("dev.test.long{n}"),
                "Usage",
                Some(&command),
                long,
            );
        }
    });
    cx.simulate_window_resize(window, size(px(720.), px(800.)));
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "activity-plugin-dev.test.long1"));
    assert_log_toggle_inside(cx, window);
}

#[gpui_kit::test]
fn a_long_install_source_keeps_the_log_toggle_inside_the_window(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |_| {});
    cx.run_until_parked();
    let folder = data.path().join("w".repeat(150));
    let _install = start_install(cx, &folder);
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    cx.simulate_window_resize(window, size(px(528.), px(720.)));
    cx.run_until_parked();
    assert!(shows(cx, window, "activity-install"));
    assert_log_toggle_inside(cx, window);
}
