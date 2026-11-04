use std::cell::RefCell;
use std::rc::Rc;

use crate::kagan::commands::{found as commands_found, state as commands_state};
use crate::kagan::{Kagan, screen_read};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{KeymapOp, Op, OpResult, Plugin, PluginId, ThreadId, ThreadOp, UiOp, WindowOp};

use super::kagan::{
    chord, click, grid_text, install_navigator, label, open, open_with_plugins_then, plugin_log,
    press, press_key, release, run_shell, shown, shows, thread_record, value_of, wait_until,
};
use super::panels::disabled_of;

fn install_header(data: &std::path::Path) {
    install_header_plugin(
        data,
        "dev.kagan.header",
        "Header",
        &[
            ("handoff", "Hand off", true, Some("secondary-shift-h")),
            (
                "ship",
                "Ship it now please",
                true,
                Some("secondary-shift-u"),
            ),
            ("quiet", "Quiet", false, None),
        ],
    );
}

fn install_header_plugin(
    data: &std::path::Path,
    id: &str,
    name: &str,
    commands: &[(&str, &str, bool, Option<&str>)],
) {
    let root = data.join("plugins").join(id);
    std::fs::create_dir_all(&root).unwrap();
    let commands = commands
        .iter()
        .map(|(command, title, header, key)| {
            let mut entry = serde_json::json!({
                "id": command,
                "title": title,
                "when": "thread",
                "header": header,
            });
            if let Some(key) = key {
                entry
                    .as_object_mut()
                    .unwrap()
                    .insert("key".into(), (*key).into());
            }
            entry
        })
        .collect::<Vec<_>>();
    let manifest = serde_json::json!({
        "id": id,
        "name": name,
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Header command buttons on the thread title.",
        "powers": {},
        "commands": commands,
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log } from "kagan/host";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind === "command") {
          log("info", `${job.command} ${job.threadId}`);
          reply(job.id, null);
        } else {
          fail(job.id, "Unknown job");
        }
      }
    });
  }
  render() { return ""; }
}
"#,
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root: dunce::canonicalize(&root).unwrap(),
            commit: None,
            enabled: true,
            approved: serde_json::json!({ "powers": {}, "downloads": {}, "folders": {} }),
            error: None,
        })
        .unwrap();
}

fn header_id(plugin: &str, command: &str) -> ElementId {
    crate::kagan::header::id_for(&PluginId(plugin.into()), command)
}

fn header_name(plugin: &str, command: &str) -> String {
    match crate::kagan::header::id_for(&PluginId(plugin.into()), command) {
        ElementId::Name(name) => name.to_string(),
        other => panic!("header id is a Name: {other:?}"),
    }
}

fn name_from_snapshot(debug: &str) -> Option<&str> {
    debug.strip_prefix("Name(\"")?.strip_suffix("\")")
}

fn shows_id(cx: &mut TestAppContext, window: AnyWindowHandle, id: &ElementId) -> bool {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, id).is_some_and(|element| element.visible)
    })
    .unwrap()
}

fn click_id(cx: &mut TestAppContext, window: AnyWindowHandle, id: &ElementId) {
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(window, id, cx).unwrap_or_else(|error| panic!("{id}: {error}"));
    })
    .unwrap();
    cx.run_until_parked();
}

fn hint_of_id(cx: &mut TestAppContext, window: AnyWindowHandle, id: &ElementId) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, id)
            .and_then(|element| element.hint.as_deref().map(str::to_owned))
    })
    .unwrap()
}

fn bounds_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &ElementId,
) -> Option<(f32, f32, f32, f32)> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, id).map(|element| {
            let bounds = element.bounds;
            (
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.origin.x + bounds.size.width),
                f32::from(bounds.origin.y + bounds.size.height),
            )
        })
    })
    .unwrap()
}

fn dispatch(cx: &mut TestAppContext, op: Op) -> OpResult {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let result = cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
    result
}

fn query(cx: &mut TestAppContext, window: AnyWindowHandle, kagan: &Entity<Kagan>, text: &str) {
    let state = cx
        .update(|cx| commands_state(kagan.read(cx)))
        .expect("the command list is open");
    cx.update_window(window, |_, window, cx| {
        state.update(cx, |state, cx| state.set_query(text, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
}

fn hint_of(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) -> Option<String> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.hint.as_deref().map(str::to_owned))
    })
    .unwrap()
}

fn keys(chord: &str) -> String {
    chord
        .split_whitespace()
        .map(|key| Kbd::format(&kg_core::parse_key(key).unwrap()))
        .collect::<Vec<_>>()
        .join(" ")
}

fn terminal_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_focused(kagan.read(cx), window, cx)
    })
    .unwrap()
}

fn rename(cx: &mut TestAppContext, id: &ThreadId, title: &str) {
    dispatch(
        cx,
        Op::Thread(ThreadOp::Rename {
            id: id.clone(),
            title: title.into(),
        }),
    );
}

fn type_line(cx: &mut TestAppContext, id: &ThreadId, text: &str) {
    dispatch(
        cx,
        Op::Thread(ThreadOp::Input {
            id: id.clone(),
            text: format!("{text}\n"),
            terminal: None,
        }),
    );
}

#[gpui_kit::test]
fn the_command_list_filters_runs_a_command_and_closes(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("OpenCommandList"));
    cx.run_until_parked();
    assert!(shows(cx, window, "command-list"));
    query(cx, window, &kagan, "new thread");
    assert!(shows(cx, window, "command-NewThread"));
    assert!(!shows(cx, window, "command-OpenSettings"));
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert!(!shows(cx, window, "command-list"));
    assert!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_some()
    );
}

#[gpui_kit::test]
fn the_rail_and_the_op_open_the_command_list(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "command-list-open");
    cx.run_until_parked();
    assert!(shows(cx, window, "command-list"));
    press_key(cx, window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "command-list"));
    dispatch(cx, Op::Window(WindowOp::CommandList));
    assert!(shows(cx, window, "command-list"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn escape_closes_the_list_with_text_typed_and_gives_the_terminal_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(terminal_focused(cx, window, &kagan));
    cx.simulate_keystrokes(window, &chord("OpenCommandList"));
    cx.run_until_parked();
    assert!(!terminal_focused(cx, window, &kagan));
    query(cx, window, &kagan, "stop");
    press_key(cx, window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "command-list"));
    assert!(terminal_focused(cx, window, &kagan));
}

#[gpui_kit::test]
fn a_command_that_needs_a_thread_is_greyed_with_the_reason(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "stop thread");
    assert_eq!(disabled_of(cx, window, "command-StopThread"), Some(true));
    assert_eq!(
        hint_of(cx, window, "command-StopThread").as_deref(),
        Some("Open a thread first.")
    );
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert!(shows(cx, window, "command-list"));
}

#[gpui_kit::test]
fn a_command_shows_its_saved_key(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    dispatch(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: "NewThread".into(),
            key: "secondary-j".into(),
            reset: false,
        }),
    );
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "new thread");
    assert_eq!(
        value_of(cx, window, "command-NewThread"),
        Some(keys("secondary-j"))
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_plugin_command_shows_under_its_plugin_with_its_key(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "show");
    assert_eq!(
        value_of(cx, window, "command-dev.kagan.navigator-show"),
        Some(keys("secondary-shift-y"))
    );
    query(cx, window, &kagan, "probe");
    assert_eq!(
        value_of(cx, window, "command-dev.kagan.navigator-probe"),
        Some(String::new())
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn thread_search_finds_a_title_and_terminal_text_without_escape_codes(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let titled = run_shell(cx);
    rename(cx, &titled, "Alpha build");
    let printed = run_shell(cx);
    type_line(cx, &printed, "printf '\\033[31mRED\\033[0mTEXT-%s\\n' done");
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).grid(&printed, None))
            .is_some_and(|grid| {
                grid.lines.iter().any(|line| {
                    line.iter()
                        .map(|cell| cell.c)
                        .collect::<String>()
                        .contains("REDTEXT-done")
                })
            })
    });

    cx.simulate_keystrokes(window, &chord("SearchThreads"));
    cx.run_until_parked();
    query(cx, window, &kagan, "ALPHA");
    wait_until(cx, |cx| shows(cx, window, &format!("found-{}", titled.0)));

    query(cx, window, &kagan, "redtext-done");
    let row = format!("found-{}", printed.0);
    wait_until(cx, |cx| shows(cx, window, &row));
    let snippet = value_of(cx, window, &row).unwrap();
    assert!(snippet.contains("REDTEXT-done"), "{snippet}");
    assert!(!snippet.contains('\u{1b}'), "{snippet:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_archived_thread_is_found_under_its_own_heading(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let live = run_shell(cx);
    rename(cx, &live, "Bravo live");
    let archived = run_shell(cx);
    rename(cx, &archived, "Bravo old");
    dispatch(
        cx,
        Op::Thread(ThreadOp::Archive {
            id: archived.clone(),
        }),
    );
    wait_until(cx, |cx| thread_record(cx, &archived).archived_at.is_some());
    dispatch(cx, Op::Window(WindowOp::SearchThreads));
    query(cx, window, &kagan, "bravo");
    wait_until(cx, |cx| shows(cx, window, &format!("found-{}", archived.0)));
    assert_eq!(
        cx.update(|cx| commands_found(kagan.read(cx))),
        Some(vec![(live, false), (archived, true)])
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_opens_the_thread_at_the_matched_line(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    type_line(cx, &id, "echo FIRST-''MARK; seq 1 300");
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).grid(&id, None))
            .is_some_and(|grid| {
                grid.lines
                    .iter()
                    .any(|line| line.iter().map(|cell| cell.c).collect::<String>().trim() == "300")
            })
    });
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{}", other.0));
    dispatch(cx, Op::Window(WindowOp::SearchThreads));
    query(cx, window, &kagan, "first-mark");
    let row = format!("found-{}", id.0);
    wait_until(cx, |cx| shows(cx, window, &row));
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shown(cx, &kagan).as_ref() == Some(&id));
    assert!(!shows(cx, window, "command-list"));
    wait_until(cx, |cx| grid_text(cx, &kagan).contains("FIRST-MARK"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_new_query_never_shows_the_old_results(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let first = run_shell(cx);
    rename(cx, &first, "Charlie one");
    let second = run_shell(cx);
    rename(cx, &second, "Delta two");
    dispatch(cx, Op::Window(WindowOp::SearchThreads));
    let seen: Rc<RefCell<Vec<ThreadId>>> = Rc::default();
    let _watch = cx.update(|cx| {
        let seen = seen.clone();
        cx.observe(&kagan, move |kagan, cx| {
            if let Some(found) = commands_found(kagan.read(cx)) {
                seen.borrow_mut()
                    .extend(found.into_iter().map(|(id, _)| id));
            }
        })
    });
    let state = cx.update(|cx| commands_state(kagan.read(cx))).unwrap();
    // Each update runs the query callback, but no search task runs until the executor parks.
    for text in ["charlie", "delta"] {
        cx.update_window(window, |_, window, cx| {
            state.update(cx, |state, cx| state.set_query(text, window, cx));
        })
        .unwrap();
    }
    wait_until(cx, |cx| shows(cx, window, &format!("found-{}", second.0)));
    assert!(!seen.borrow().contains(&first), "{:?}", seen.borrow());
}

#[cfg(unix)]
#[gpui_kit::test]
fn choosing_a_plugin_command_runs_it_with_the_shown_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    let _other = run_shell(cx);
    let first = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "show");
    click(cx, window, "command-dev.kagan.navigator-show");
    wait_until(cx, |cx| {
        shown(cx, &kagan).is_some_and(|shown| shown != first)
    });
    assert!(plugin_log(cx, "dev.kagan.navigator").contains("show ran"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_plugin_key_another_command_took_shows_no_key(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "clash");
    assert_eq!(
        value_of(cx, window, "command-dev.kagan.navigator-clash"),
        Some(String::new())
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_command_that_keeps_focus_gives_the_terminal_back_after_enter(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "bigger terminal");
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert!(!shows(cx, window, "command-list"));
    assert!(terminal_focused(cx, window, &kagan));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_click_outside_closes_the_list_and_gives_the_terminal_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    dispatch(cx, Op::Window(WindowOp::CommandList));
    assert!(shows(cx, window, "command-list"));
    let at = gpui_kit::point(gpui_kit::px(700.), gpui_kit::px(650.));
    press(cx, window, at, 1, gpui_kit::Modifiers::none());
    release(cx, window, at, 1, gpui_kit::Modifiers::none());
    cx.run_until_parked();
    assert!(!shows(cx, window, "command-list"));
    assert!(terminal_focused(cx, window, &kagan));
}

#[gpui_kit::test]
fn a_failed_search_says_why_instead_of_no_match(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    dispatch(cx, Op::Window(WindowOp::SearchThreads));
    query(
        cx,
        window,
        &kagan,
        &"x".repeat(kg_domain::SEARCH_TEXT_MAX + 1),
    );
    wait_until(cx, |cx| {
        label(cx, window, "command-threads-empty").starts_with("Search failed:")
    });
    assert!(label(cx, window, "command-threads-empty").contains("longer than"));
}

#[gpui_kit::test]
fn typing_a_plugin_name_shows_its_commands(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_navigator);
    dispatch(cx, Op::Window(WindowOp::CommandList));
    query(cx, window, &kagan, "navigator");
    assert!(shows(cx, window, "command-dev.kagan.navigator-show"));
    assert!(!shows(cx, window, "command-NewThread"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_header_command_is_a_button_on_the_thread_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_header);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let handoff = header_id("dev.kagan.header", "handoff");
    let quiet = header_id("dev.kagan.header", "quiet");
    wait_until(cx, |cx| shows_id(cx, window, &handoff));
    assert!(!shows_id(cx, window, &quiet));
    click_id(cx, window, &handoff);
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.kagan.header").contains(&format!("handoff {}", id.0))
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_header_tooltip_shows_that_command_s_shortcut(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_header);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let handoff = header_id("dev.kagan.header", "handoff");
    let ship = header_id("dev.kagan.header", "ship");
    wait_until(cx, |cx| shows_id(cx, window, &handoff));
    let handoff_keys = keys("secondary-shift-h");
    let ship_keys = keys("secondary-shift-u");
    assert_eq!(
        hint_of_id(cx, window, &handoff),
        Some(format!("Hand off ({handoff_keys})"))
    );
    assert_eq!(
        hint_of_id(cx, window, &ship),
        Some(format!("Ship it now please ({ship_keys})"))
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn header_buttons_do_not_cover_the_thread_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_header);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let handoff = header_id("dev.kagan.header", "handoff");
    wait_until(cx, |cx| shows_id(cx, window, &handoff));
    rename(
        cx,
        &id,
        &"Very long thread title that should ellipsize".repeat(3),
    );
    cx.run_until_parked();
    let title = bounds_of(cx, window, &ElementId::Name("thread-title".into())).expect("title");
    let heading =
        bounds_of(cx, window, &ElementId::Name("thread-heading".into())).expect("heading");
    let button = bounds_of(cx, window, &handoff).expect("handoff");
    assert!(
        title.2 <= button.0 + 0.5 && heading.2 <= button.0 + 0.5,
        "title {title:?} heading {heading:?} reaches into button {button:?}"
    );
    assert!(
        button.2 - button.0 <= 120.5,
        "button wider than cap: {button:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn hyphen_joined_header_ids_stay_distinct(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, |data| {
        install_header_plugin(
            data,
            "dev.acme.header",
            "Header",
            &[("hand-off", "One", true, None)],
        );
        install_header_plugin(
            data,
            "dev.acme.header-hand",
            "Header hand",
            &[("off", "Two", true, None)],
        );
    });
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let one = header_id("dev.acme.header", "hand-off");
    let two = header_id("dev.acme.header-hand", "off");
    wait_until(cx, |cx| {
        shows_id(cx, window, &one) && shows_id(cx, window, &two)
    });
    assert_ne!(one, two);
    assert_eq!(
        header_name("dev.acme.header", "hand-off"),
        "header/dev.acme.header/hand-off"
    );
    assert_eq!(
        header_name("dev.acme.header-hand", "off"),
        "header/dev.acme.header-hand/off"
    );
    let OpResult::Ui(snapshot) = dispatch(cx, Op::Ui(UiOp::Snapshot { window: 0 })) else {
        panic!("snapshot");
    };
    let from_snapshot = |want: &str| {
        snapshot
            .elements
            .iter()
            .find_map(|element| {
                let name = name_from_snapshot(&element.id)?;
                (name == want).then(|| name.to_owned())
            })
            .unwrap_or_else(|| {
                panic!(
                    "snapshot lists {want}: {:?}",
                    snapshot.elements.iter().map(|e| &e.id).collect::<Vec<_>>()
                )
            })
    };
    let click_one = from_snapshot("header/dev.acme.header/hand-off");
    let click_two = from_snapshot("header/dev.acme.header-hand/off");
    assert!(matches!(
        dispatch(
            cx,
            Op::Ui(UiOp::Click {
                element_id: click_one,
                within: None,
                window: 0,
            }),
        ),
        OpResult::Done
    ));
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.acme.header").contains(&format!("hand-off {}", id.0))
    });
    assert!(matches!(
        dispatch(
            cx,
            Op::Ui(UiOp::Click {
                element_id: click_two,
                within: None,
                window: 0,
            }),
        ),
        OpResult::Done
    ));
    wait_until(cx, |cx| {
        plugin_log(cx, "dev.acme.header-hand").contains(&format!("off {}", id.0))
    });
}

#[gpui_kit::test]
fn the_panels_toggle_stays_on_the_right_without_a_thread_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_window_resize(window, size(px(1100.), px(720.)));
    cx.run_until_parked();
    let (left_x, left_w) =
        super::sidebars::span(cx, window, "left-sidebar-toggle").expect("left toggle");
    let (right_x, right_w) =
        super::sidebars::span(cx, window, "right-sidebar-toggle").expect("right toggle");
    // Windows keeps caption buttons in the title bar, so the toggle sits left of 1000
    // in an 1100-wide window; the spacer still has to push it well past the left toggle.
    assert!(
        right_x > left_x + left_w + 100. && right_x + right_w > 900.,
        "left {left_x}+{left_w} right {right_x}+{right_w}"
    );
}
