use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{
    InteractionOption, Op, OpError, OpResult, Plugin, PluginId, PluginOp, PreferenceOp, Slot,
    ThreadId, ThreadOp,
};
use serde_json::json;

#[cfg(unix)]
use crate::kagan::review_read;
use crate::kagan::{Kagan, replace_read};
#[cfg(unix)]
use crate::tests::kagan::repo_thread;
use crate::tests::kagan::{
    chord, click, label, open_with_plugins_then, pending, plugin_log, present, run_shell, shows,
    wait_until,
};

const SWAP: &str = "dev.test.swap";

const LIST: &str = r#"return div()
      .child(new KgButton("swap-list").label("Swap list"))
      .child(new KgButton("fake-approval").label("Allow"))
      .children(this.names.map((name, ix) => new KgButton("swap-row-" + ix).label(name)));"#;

const THROWS: &str = r#"throw new Error("list exploded");"#;

const GRABS: &str = r#"return div()
      .track_focus(this.focus)
      .on_key_down((_event, cx) => cx.stop_propagation())
      .child(new KgButton("swap-list").label("Swap list"));"#;

const POPS: &str = r#"return div()
      .child(new KgButton("swap-list").label("Swap list"))
      .child(Popover.new("swap-pop").open(true)
        .trigger(new KgButton("swap-pop-open").label("Open"))
        .content(new KgButton("fake-approval").label("Allow")));"#;

const COVERS: &str = r#"return div()
      .relative()
      .size_full()
      .child(new KgButton("swap-list").label("Swap list " + String(this.taken || 0)))
      .child(div().absolute().left(0).top(0).w(4000).h(4000).on_mouse_down("left", (_event, cx) => {
        this.taken = (this.taken || 0) + 1;
        cx.stop_propagation();
      }));"#;

#[cfg(unix)]
const DIFF_COVERS: &str = r#"return div()
      .relative()
      .size_full()
      .child(new KgButton("swap-diff-path").label(this.path || "none"))
      .child(div().absolute().left(-3000).top(-3000).w(9000).h(9000).on_mouse_down("left", (_event, cx) => {
        cx.stop_propagation();
      }));"#;

const TIPS: &str =
    r#"return div().child(new KgButton("swap-list").label("Swap list").tooltip("Allow"));"#;

const POPUP: &str = r#"return div()
      .child(new KgButton("swap-list").label("Swap list"))
      .child(Popup.new("swap-popup", new KgButton("swap-popup-open").label("Open"))
        .content(new KgButton("fake-approval").label("Allow")));"#;

const DIALOG: &str = r#"return div()
      .child(new KgButton("swap-list").label("Swap list"))
      .child(new Dialog("swap-dialog", "Open", () => {}).title("Allow?"));"#;

const LATE: &str = r#"if (!this.later) {
      this.later = true;
      cx.spawn(async (cx) => {
        await cx.sleep(200);
        this.late = true;
        this.notify();
      });
    }
    const list = div().child(new KgButton("swap-list").label("Swap list"));
    return this.late
      ? list.child(Popover.new("swap-pop").open(true)
          .trigger(new KgButton("swap-pop-open").label("Open"))
          .content(new KgButton("fake-approval").label("Allow")))
      : list;"#;

const LOOPS: &str = "for (;;) {}";

const DIFF: &str = r#"return div()
      .child(new KgButton("swap-diff-path").label(this.path || "none"))
      .child(new KgButton("swap-diff-files").label(this.files || "none"))
      .child(new KgButton("swap-said").label(this.said || "none"))
      .child(new KgButton("swap-comment").label("Comment").on_click(() => this.comment(this.path, "from the plugin")))
      .child(new KgButton("swap-comment-elsewhere").label("Elsewhere").on_click(() => this.comment("nope.txt", "elsewhere")))
      .child(new KgButton("swap-comment-long").label("Long").on_click(() => this.comment(this.path, "x".repeat(5000))));"#;

#[cfg(unix)]
const DIFF_GRABS: &str = r#"return div()
      .track_focus(this.focus)
      .on_key_down((_event, cx) => cx.stop_propagation())
      .child(new KgButton("swap-diff-path").label(this.path || "none"));"#;

const GRAB_EVERY_FRAME: &str = r"cx.spawn(async (cx) => {
      for (;;) {
        this.focus.focus();
        await cx.sleep(16);
      }
    });";

fn source(list: &str) -> String {
    source_with(list, DIFF)
}

fn source_with(list: &str, diff: &str) -> String {
    let grab = |body: &str| {
        if body.contains("track_focus") {
            GRAB_EVERY_FRAME
        } else {
            ""
        }
    };
    let (grab_list, grab_diff) = (grab(list), grab(diff));
    format!(
        r#"// @ts-check
import {{ View, div }} from "gpui-kit";
import {{ Popover, Popup }} from "gpui-base";
import {{ Dialog, KgButton }} from "gpui-component";
import {{ changes, sidebar }} from "kagan/threads";
import {{ addComment, context, onContextChanged }} from "kagan/host";
import {{ fail, next, reply }} from "kagan/jobs";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        Promise.resolve().then(() => changes(job.argv[0], "uncommitted")).then(
          (found) => reply(job.id, {{ exitCode: 0, stdout: found.files.map((file) => file.path).join(","), stderr: "" }}),
          (error) => reply(job.id, {{ exitCode: 1, stdout: "", stderr: String(error) }}),
        ).catch((error) => fail(job.id, String(error)));
      }}
    }});
  }}
  render() {{ return ""; }}
}}
export class ListPanel extends View {{
  init(_props, cx) {{
    this.focus = cx.focus_handle();
    this.notify = () => cx.notify();
    this.names = [];
    const read = () => {{ this.names = sidebar().threads.map((thread) => thread.title); }};
    read();
    {grab_list}
    cx.spawn(async () => {{
      for (;;) {{
        await onContextChanged();
        read();
        cx.notify();
      }}
    }});
  }}
  render(cx) {{
    {list}
  }}
}}
export class DiffPanel extends View {{
  init(_props, cx) {{
    this.focus = cx.focus_handle();
    this.path = "";
    this.files = "";
    this.said = "";
    this.comment = (path, text) => {{
      Promise.resolve().then(() => addComment({{ path, start: 2, end: 2, text }})).then(
        () => {{ this.said = "ok"; cx.notify(); }},
        (error) => {{ this.said = String(error); cx.notify(); }},
      );
    }};
    const read = (value) => {{
      const props = value && value.props;
      if (!props || !props.threadId) return;
      this.path = props.path;
      changes(props.threadId, props.scope).then(
        (found) => {{ this.files = found.files.map((file) => file.path).join(","); cx.notify(); }},
        (error) => {{ this.files = String(error); cx.notify(); }},
      );
    }};
    read(context());
    {grab_diff}
    cx.spawn(async () => {{
      for (;;) {{
        read(await onContextChanged());
        cx.notify();
      }}
    }});
  }}
  render() {{
    {diff}
  }}
}}
"#
    )
}

fn install(data: &Path, folder: &str, id: &str, list: &str) {
    install_with(data, folder, id, &source(list));
}

fn install_with(data: &Path, folder: &str, id: &str, entry: &str) {
    let root = data.join(folder);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("kagan.json"), manifest(id).to_string()).unwrap();
    std::fs::write(root.join("main.js"), entry).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {"areas": ["threads"]}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

fn manifest(id: &str) -> serde_json::Value {
    json!({
        "id": id,
        "name": "Swap",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Draws the thread list and the review.",
        "powers": {"areas": ["threads"]},
        "cli": [{"name": "swap", "commands": [{"name": "changes", "usage": "changes <thread>"}]}],
        "ui": {"panels": [
            {"id": "list", "title": "Swap list", "view": "ListPanel", "placement": "left", "replaces": "threads"},
            {"id": "diff", "title": "Swap diff", "view": "DiffPanel", "placement": "center", "replaces": "review"}
        ]}
    })
}

fn opened(
    cx: &mut TestAppContext,
    list: &'static str,
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let (kagan, window, data) =
        open_with_plugins_then(cx, |data| install(data, "swap", SWAP, list));
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    (kagan, window, data)
}

fn run(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task)
}

fn choose(cx: &mut TestAppContext, slot: Slot, id: &str) {
    run(
        cx,
        Op::Plugin(PluginOp::Use {
            slot,
            id: id.parse().unwrap(),
            thread_id: None,
        }),
    )
    .unwrap();
    cx.run_until_parked();
}

fn saved(cx: &mut TestAppContext, slot: Slot) -> serde_json::Value {
    match run(
        cx,
        Op::Preference(PreferenceOp::Get {
            key: slot.preference().to_owned(),
        }),
    )
    .unwrap()
    {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn plugin_op(cx: &mut TestAppContext, op: PluginOp) {
    run(cx, Op::Plugin(op)).unwrap();
    cx.run_until_parked();
}

fn filled(cx: &mut TestAppContext, kagan: &Entity<Kagan>, slot: Slot) -> bool {
    cx.update(|cx| replace_read::filled(kagan.read(cx), slot))
}

fn bounds(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) -> gpui_kit::Bounds<f32> {
    cx.update_window(window, |_, window, _| {
        let bounds = crate::ui_registry::find(window, &ElementId::Name(id.to_owned().into()))
            .unwrap_or_else(|| panic!("{id} is not drawn"))
            .bounds;
        gpui_kit::Bounds::new(
            gpui_kit::point(f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
            gpui_kit::size(f32::from(bounds.size.width), f32::from(bounds.size.height)),
        )
    })
    .unwrap()
}

#[gpui_kit::test]
fn only_the_persons_choice_switches_a_slot(cx: &mut TestAppContext) {
    let (kagan, window, data) = opened(cx, LIST);
    let id = PluginId(SWAP.into());
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(!shows(cx, window, "swap-list"));
    assert_eq!(saved(cx, Slot::Threads), serde_json::Value::Null);
    let refused = run(
        cx,
        Op::Plugin(PluginOp::Use {
            slot: Slot::Threads,
            id: "dev.test.nobody".parse().unwrap(),
            thread_id: None,
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(
        refused.contains("does not offer a replaceable sidebar"),
        "{refused}"
    );
    let escape = run(
        cx,
        Op::Preference(PreferenceOp::Set {
            key: Slot::Threads.preference().to_owned(),
            value: json!("../../outside"),
        }),
    )
    .unwrap_err()
    .to_string();
    assert!(escape.contains("must be a plugin id or kagan"), "{escape}");

    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    assert!(!shows(cx, window, "project-prj_kagan"));

    plugin_op(
        cx,
        PluginOp::Reload {
            id: id.clone(),
            thread_id: None,
        },
    );
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    assert_eq!(saved(cx, Slot::Threads), json!(SWAP));

    plugin_op(cx, PluginOp::Disable { id: id.clone() });
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert!(plugin_log(cx, SWAP).contains("The replaceable sidebar is back to built-in"));
    plugin_op(cx, PluginOp::Enable { id: id.clone() });
    cx.run_until_parked();
    assert!(!shows(cx, window, "swap-list"));
    assert!(!filled(cx, &kagan, Slot::Threads));
    assert_eq!(saved(cx, Slot::Threads), json!("kagan"));

    let other = data.path().join("other");
    std::fs::create_dir(&other).unwrap();
    std::fs::write(
        other.join("kagan.json"),
        manifest("dev.test.other").to_string(),
    )
    .unwrap();
    std::fs::write(other.join("main.js"), source(LIST)).unwrap();
    let installing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source: other.display().to_string(),
                plugin: None,
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
    cx.foreground_executor().block_test(installing).unwrap();
    cx.run_until_parked();
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(!filled(cx, &kagan, Slot::Threads));
    assert_eq!(saved(cx, Slot::Threads), json!("kagan"));
}

#[gpui_kit::test]
fn a_sidebar_plugin_draws_only_the_list_and_the_host_keeps_questions(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, LIST);
    let thread = run_shell(cx);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-row-0"));
    assert!(shows(cx, window, "fake-approval"));
    assert!(shows(cx, window, "threads-home"));
    assert!(shows(cx, window, "settings-gear"));
    assert!(shows(cx, window, "sidebar-new-thread"));
    assert!(shows(cx, window, "kagan-menu"));
    assert!(!shows(cx, window, &format!("thread-{}", thread.0)));

    let _asking = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: thread.clone(),
                title: "Keep going?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                        role: None,
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                        role: None,
                    },
                ],
                fields: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| !pending(cx, &thread).is_empty());
    let question = pending(cx, &thread).remove(0);
    let card = format!("question-{}", question.id.0);
    wait_until(cx, |cx| shows(cx, window, &card));
    let list = bounds(cx, window, "replaced-threads");
    assert_eq!(label(cx, window, "rail-waiting"), "1 waiting");
    assert!(!list.intersects(&bounds(cx, window, "rail-waiting")));
    let real = bounds(cx, window, &card);
    assert!(
        !list.intersects(&real),
        "the host's question {real:?} sits inside the plugin's list {list:?}"
    );
    assert!(list.intersects(&bounds(cx, window, "fake-approval")));
    click(cx, window, &format!("answer-{}-yes", question.id.0));
    wait_until(cx, |cx| pending(cx, &thread).is_empty());
}

#[gpui_kit::test]
fn a_sidebar_that_throws_gives_the_built_in_back_and_says_why(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, THROWS);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| saved(cx, Slot::Threads) == json!("kagan"));
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(!filled(cx, &kagan, Slot::Threads));
    let log = plugin_log(cx, SWAP);
    assert!(
        log.contains("The replaceable sidebar is back to built-in: its view threw"),
        "{log}"
    );
    assert!(log.contains("list exploded"), "{log}");
}

#[gpui_kit::test]
fn the_built_in_key_works_while_the_plugin_holds_focus(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, GRABS);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    wait_until(cx, |cx| {
        cx.update_window(window, |_, window, cx| {
            replace_read::holds_focus(kagan.read(cx), Slot::Threads, window, cx)
        })
        .unwrap()
    });
    cx.simulate_keystrokes(window, &chord("UseBuiltInSidebar"));
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert!(!filled(cx, &kagan, Slot::Threads));
    assert!(
        cx.update_window(window, |_, window, cx| {
            replace_read::window_holds_keys(kagan.read(cx), window, cx)
        })
        .unwrap()
    );
    assert_eq!(saved(cx, Slot::Threads), json!("kagan"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_review_plugin_adds_notes_that_only_the_host_sends(cx: &mut TestAppContext) {
    let (kagan, window, data) = opened(cx, LIST);
    let (_thread, _repo) = repo_thread(cx, window, data.path());
    choose(cx, Slot::Review, SWAP);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-files") == "notes.txt");
    assert_eq!(label(cx, window, "swap-diff-path"), "notes.txt");
    assert!(!shows(cx, window, "comment-New-2"));
    assert!(shows(cx, window, "review-discard"));
    click(cx, window, "swap-comment");
    wait_until(cx, |cx| {
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))) == 1
    });
    click(cx, window, "review-send");
    wait_until(cx, |cx| !shows(cx, window, "review"));
    wait_until(cx, |cx| {
        crate::tests::kagan::grid_text(cx, &kagan).contains("from the plugin")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_review_opened_again_tells_a_reloaded_plugin_its_file(cx: &mut TestAppContext) {
    let (_kagan, window, data) = opened(cx, LIST);
    let (thread, _repo) = repo_thread(cx, window, data.path());
    let other = run_shell(cx);
    choose(cx, Slot::Review, SWAP);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
    click(cx, window, &format!("thread-{}", other.0));
    wait_until(cx, |cx| !shows(cx, window, "review"));
    plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(SWAP.into()),
            thread_id: None,
        },
    );
    click(cx, window, &format!("thread-{}", thread.0));
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
}

#[cfg(unix)]
#[gpui_kit::test]
fn settings_picks_the_review_and_the_key_takes_it_back(cx: &mut TestAppContext) {
    let (kagan, window, data) = opened(cx, LIST);
    let _ = repo_thread(cx, window, data.path());
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
    let note = label(cx, window, &format!("setting-row-{SWAP}-note"));
    assert!(
        note.contains("offers a replaceable sidebar")
            && note.contains("offers a replaceable review"),
        "{note}"
    );
    assert_eq!(label(cx, window, "slot-review"), "Built-in");
    let choice = format!("slot-review-{SWAP}");
    assert!(
        !shows(cx, window, &choice),
        "the choices stay in the menu until it opens"
    );
    click(cx, window, "slot-review");
    wait_until(cx, |cx| shows(cx, window, &choice));
    click(cx, window, &choice);
    wait_until(cx, |cx| saved(cx, Slot::Review) == json!(SWAP));
    wait_until(cx, |cx| label(cx, window, "slot-review") == "Swap");
    wait_until(cx, |cx| filled(cx, &kagan, Slot::Review));
    click(cx, window, "settings-gear");
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
    click(cx, window, "swap-comment");
    cx.simulate_keystrokes(window, &chord("UseBuiltInReview"));
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    assert!(!shows(cx, window, "swap-diff-path"));
    assert_eq!(saved(cx, Slot::Review), json!("kagan"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_plugin_reads_changes_only_of_listed_threads(cx: &mut TestAppContext) {
    let (_kagan, window, data) = opened(cx, LIST);
    let (thread, repo) = repo_thread(cx, window, data.path());
    // git runs outside the test clock, so the answer is polled rather than blocked on.
    let probe = |cx: &mut TestAppContext, thread: &ThreadId| {
        let answer = std::rc::Rc::new(std::cell::RefCell::new(None));
        let slot = answer.clone();
        let op = Op::Plugin(PluginOp::Cli {
            name: "swap".into(),
            command: "changes".into(),
            argv: vec![thread.0.clone()],
            cwd: repo.clone(),
            thread_id: None,
            project_id: None,
        });
        cx.update(|cx| {
            let task = Workspace::dispatch(op, cx);
            cx.spawn(async move |_| *slot.borrow_mut() = Some(task.await))
                .detach();
        });
        wait_until(cx, |_| answer.borrow().is_some());
        answer.take().unwrap().unwrap()
    };
    let OpResult::Cli { stdout, .. } = probe(cx, &thread) else {
        panic!("cli");
    };
    assert_eq!(stdout, "notes.txt");
    run(cx, Op::Thread(ThreadOp::Archive { id: thread.clone() })).unwrap();
    cx.run_until_parked();
    let OpResult::Cli { stderr, .. } = probe(cx, &thread) else {
        panic!("cli");
    };
    assert!(stderr.contains("is not in kagan's thread list"), "{stderr}");
}

#[cfg(unix)]
fn install_example(data: &Path, name: &str) {
    let from = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/plugins")
        .join(name);
    let root = data.join(name);
    std::fs::create_dir(&root).unwrap();
    for file in ["kagan.json", "main.js", "icon.svg"] {
        std::fs::copy(from.join(file), root.join(file)).unwrap();
    }
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("kagan.json")).unwrap()).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(manifest["id"].as_str().unwrap().into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {"areas": ["threads"]}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_bundled_examples_fill_both_slots(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, |data| {
        install_example(data, "sidebar");
        install_example(data, "review");
    });
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    let (thread, _repo) = repo_thread(cx, window, data.path());
    choose(cx, Slot::Review, "dev.kagan.plain-diff");
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "plain-diff-2") == "+two");
    click(cx, window, "plain-diff-2");
    wait_until(cx, |cx| {
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))) == 1
    });
    choose(cx, Slot::Threads, "dev.kagan.plain-list");
    let row = format!("plain-list-{}", thread.0);
    wait_until(cx, |cx| shows(cx, window, &row));
    assert!(filled(cx, &kagan, Slot::Threads));
}

#[cfg(unix)]
fn opened_with(
    cx: &mut TestAppContext,
    entry: String,
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let (kagan, window, data) =
        open_with_plugins_then(cx, move |data| install_with(data, "swap", SWAP, &entry));
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    (kagan, window, data)
}

fn back_to_built_in(cx: &mut TestAppContext, slot: Slot, because: &str) {
    wait_until(cx, |cx| saved(cx, slot) == json!("kagan"));
    let log = plugin_log(cx, SWAP);
    assert!(
        log.contains(&format!("The {} is back to built-in", slot.name())) && log.contains(because),
        "{log}"
    );
}

fn ask(cx: &mut TestAppContext, thread: &ThreadId) -> kg_domain::PendingInteraction {
    let _asking = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Ask {
                id: thread.clone(),
                title: "Keep going?".into(),
                options: vec![
                    InteractionOption {
                        id: "yes".into(),
                        label: "Yes".into(),
                        role: None,
                    },
                    InteractionOption {
                        id: "no".into(),
                        label: "No".into(),
                        role: None,
                    },
                ],
                fields: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| !pending(cx, thread).is_empty());
    pending(cx, thread).remove(0)
}

fn answer_standalone(cx: &mut TestAppContext, option: &str) {
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
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, option.into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
}

#[gpui_kit::test]
fn a_replacement_that_floats_goes_back(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, POPS);
    choose(cx, Slot::Threads, SWAP);
    back_to_built_in(cx, Slot::Threads, "uses Popover");
    assert!(shows(cx, window, "project-prj_kagan"));
    assert!(!shows(cx, window, "fake-approval"));
    assert!(!filled(cx, &kagan, Slot::Threads));
}

#[gpui_kit::test]
fn a_replacement_with_a_tooltip_goes_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, TIPS);
    choose(cx, Slot::Threads, SWAP);
    back_to_built_in(cx, Slot::Threads, "uses Tooltip");
    assert!(shows(cx, window, "project-prj_kagan"));
}

#[gpui_kit::test]
fn a_cover_cannot_take_clicks_outside_its_frame(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, COVERS);
    let thread = run_shell(cx);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    let question = ask(cx, &thread);
    let answer = format!("answer-{}-yes", question.id.0);
    wait_until(cx, |cx| shows(cx, window, &answer));
    click(cx, window, &answer);
    wait_until(cx, |cx| pending(cx, &thread).is_empty());
    assert_eq!(label(cx, window, "swap-list"), "Swap list 0");
}

#[gpui_kit::test]
fn a_view_that_never_finishes_drawing_goes_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, LOOPS);
    choose(cx, Slot::Threads, SWAP);
    back_to_built_in(cx, Slot::Threads, "its view threw");
    assert!(shows(cx, window, "project-prj_kagan"));
}

#[gpui_kit::test]
fn a_plugin_that_stops_offering_or_starting_gives_the_built_in_back(cx: &mut TestAppContext) {
    let (_kagan, window, data) = opened(cx, LIST);
    let id = PluginId(SWAP.into());
    let reload = |cx: &mut TestAppContext| {
        let _ = run(
            cx,
            Op::Plugin(PluginOp::Reload {
                id: id.clone(),
                thread_id: None,
            }),
        );
        cx.run_until_parked();
    };

    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    let mut dropped = manifest(SWAP);
    dropped["ui"]["panels"][0]
        .as_object_mut()
        .unwrap()
        .remove("replaces");
    std::fs::write(data.path().join("swap/kagan.json"), dropped.to_string()).unwrap();
    reload(cx);
    back_to_built_in(cx, Slot::Threads, "no longer offers it");

    std::fs::write(
        data.path().join("swap/kagan.json"),
        manifest(SWAP).to_string(),
    )
    .unwrap();
    reload(cx);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    std::fs::write(data.path().join("swap/main.js"), "export default (").unwrap();
    reload(cx);
    wait_until(cx, |cx| saved(cx, Slot::Threads) == json!("kagan"));
    assert!(shows(cx, window, "project-prj_kagan"));
}

#[gpui_kit::test]
fn removing_the_plugin_gives_the_built_in_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, LIST);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    let removing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: PluginId(SWAP.into()),
                thread_id: None,
            }),
            cx,
        )
    });
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    wait_until(cx, |cx| shows(cx, window, "project-prj_kagan"));
    assert_eq!(saved(cx, Slot::Threads), json!("kagan"));
}

#[gpui_kit::test]
fn a_request_from_outside_the_window_asks_the_person(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, LIST);
    let (_keep, gone) = async_channel::bounded::<()>(1);
    let from_terminal = |cx: &mut TestAppContext, op: Op| {
        let gone = gone.clone();
        cx.update(|cx| Workspace::dispatch_for(op, Some(kg_core::Requester::new(gone)), cx))
    };
    let set = from_terminal(
        cx,
        Op::Preference(PreferenceOp::Set {
            key: Slot::Threads.preference().to_owned(),
            value: json!(SWAP),
        }),
    );
    let refused = cx
        .foreground_executor()
        .block_test(set)
        .unwrap_err()
        .to_string();
    assert!(
        refused.contains("kg plugin use, which asks a person"),
        "{refused}"
    );

    let choosing = Op::Plugin(PluginOp::Use {
        slot: Slot::Threads,
        id: SWAP.parse().unwrap(),
        thread_id: None,
    });
    let declined = from_terminal(cx, choosing.clone());
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());
    cx.run_until_parked();
    assert!(!filled(cx, &kagan, Slot::Threads));
    assert_eq!(saved(cx, Slot::Threads), serde_json::Value::Null);

    let approved = from_terminal(cx, choosing);
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(approved).unwrap();
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_review_key_works_while_the_review_plugin_holds_focus(cx: &mut TestAppContext) {
    let (kagan, window, data) = opened_with(cx, source_with(LIST, DIFF_GRABS));
    let _ = repo_thread(cx, window, data.path());
    choose(cx, Slot::Review, SWAP);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
    wait_until(cx, |cx| {
        cx.update_window(window, |_, window, cx| {
            replace_read::holds_focus(kagan.read(cx), Slot::Review, window, cx)
        })
        .unwrap()
    });
    cx.simulate_keystrokes(window, &chord("UseBuiltInReview"));
    wait_until(cx, |cx| shows(cx, window, "comment-New-2"));
    assert_eq!(saved(cx, Slot::Review), json!("kagan"));
    assert!(
        cx.update_window(window, |_, window, cx| {
            replace_read::window_holds_keys(kagan.read(cx), window, cx)
        })
        .unwrap()
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_comment_must_name_a_changed_file_and_stay_short(cx: &mut TestAppContext) {
    let (kagan, window, data) = opened(cx, LIST);
    let _ = repo_thread(cx, window, data.path());
    choose(cx, Slot::Review, SWAP);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
    click(cx, window, "swap-comment-elsewhere");
    wait_until(cx, |cx| label(cx, window, "swap-said") != "none");
    assert!(
        label(cx, window, "swap-said").contains("nope.txt is not a changed file"),
        "{}",
        label(cx, window, "swap-said")
    );
    click(cx, window, "swap-comment-long");
    wait_until(cx, |cx| {
        label(cx, window, "swap-said").contains("at most 4096")
    });
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(kagan.read(cx))),
        0
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_comment_goes_to_the_front_window_once(cx: &mut TestAppContext) {
    let (_first, first, data) = opened(cx, LIST);
    let (thread, _repo) = crate::tests::kagan::repo_thread(cx, first, data.path());
    let second = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[1]);
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    let _ = cx.update_window(window, |_, window, _| window.activate_window());
    click(cx, window, &format!("thread-{}", thread.0));
    wait_until(cx, |cx| {
        cx.update(|cx| {
            kg_plugins::HostUi::global(cx)
                .unwrap()
                .read(cx)
                .targets(window.window_id())
        })
    });
    choose(cx, Slot::Review, SWAP);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
    click(cx, window, "swap-comment");
    wait_until(cx, |cx| label(cx, window, "swap-said") != "none");
    assert_eq!(label(cx, window, "swap-said"), "ok");
    assert_eq!(
        cx.update(|cx| review_read::review_note_count(second.read(cx))),
        1
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_plugin_cannot_read_a_hidden_thread(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = opened(cx, LIST);
    let hidden = crate::tests::kagan::run_hidden_shell(cx);
    let answer = std::rc::Rc::new(std::cell::RefCell::new(None));
    let slot = answer.clone();
    let op = Op::Plugin(PluginOp::Cli {
        name: "swap".into(),
        command: "changes".into(),
        argv: vec![hidden.0.clone()],
        cwd: data.path().to_path_buf(),
        thread_id: None,
        project_id: None,
    });
    cx.update(|cx| {
        let task = Workspace::dispatch(op, cx);
        cx.spawn(async move |_| *slot.borrow_mut() = Some(task.await))
            .detach();
    });
    wait_until(cx, |_| answer.borrow().is_some());
    let Some(Ok(OpResult::Cli { stderr, .. })) = answer.take() else {
        panic!("cli");
    };
    assert!(stderr.contains("is not in kagan's thread list"), "{stderr}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_review_cover_cannot_take_the_hosts_clicks(cx: &mut TestAppContext) {
    let (_kagan, window, data) = opened_with(cx, source_with(LIST, DIFF_COVERS));
    let _ = repo_thread(cx, window, data.path());
    choose(cx, Slot::Review, SWAP);
    wait_until(cx, |cx| shows(cx, window, "change-notes.txt"));
    click(cx, window, "change-notes.txt");
    wait_until(cx, |cx| label(cx, window, "swap-diff-path") == "notes.txt");
    click(cx, window, "settings-gear");
    wait_until(cx, |cx| shows(cx, window, "settings-page"));
}

#[gpui_kit::test]
fn a_slot_button_clicked_by_kg_ui_needs_a_person(cx: &mut TestAppContext) {
    let (kagan, window, _data) = opened(cx, LIST);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-installed");
    wait_until(cx, |cx| present(cx, window, "slot-threads"));
    click(cx, window, "slot-threads");
    let choice = format!("slot-threads-{SWAP}");
    wait_until(cx, |cx| shows(cx, window, &choice));
    let clicked = run(
        cx,
        Op::Ui(kg_domain::UiOp::Click {
            element_id: choice,
            within: None,
            window: 0,
        }),
    );
    assert!(
        matches!(clicked, Err(OpError::Invalid(ref message)) if message == kg_domain::UI_PERSON_ONLY),
        "{clicked:?}"
    );
    cx.run_until_parked();
    assert_eq!(saved(cx, Slot::Threads), serde_json::Value::Null);
    assert!(!filled(cx, &kagan, Slot::Threads));
}

#[gpui_kit::test]
fn a_popup_or_a_dialog_in_a_replacement_goes_back(cx: &mut TestAppContext) {
    for (body, name) in [(POPUP, "uses Popup"), (DIALOG, "uses Dialog")] {
        let (_kagan, window, _data) = opened(cx, body);
        choose(cx, Slot::Threads, SWAP);
        back_to_built_in(cx, Slot::Threads, name);
        assert!(shows(cx, window, "project-prj_kagan"));
        cx.update(|cx| {
            for handle in cx.windows() {
                let _ = handle.update(cx, |_, window, _| window.remove_window());
            }
        });
        cx.run_until_parked();
    }
}

#[gpui_kit::test]
fn a_popover_opened_later_still_goes_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = opened(cx, LATE);
    choose(cx, Slot::Threads, SWAP);
    wait_until(cx, |cx| shows(cx, window, "swap-list"));
    assert_eq!(saved(cx, Slot::Threads), json!(SWAP));
    back_to_built_in(cx, Slot::Threads, "uses Popover");
    assert!(shows(cx, window, "project-prj_kagan"));
}
