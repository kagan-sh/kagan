use crate::kagan::review_read;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentSpec, Op, OpError, OpResult, Plugin, PluginId, PluginOp, Project, ProjectId, Run,
    ThreadId, ThreadOp, Visibility,
};

use crate::kagan::Kagan;

use super::{
    approvals_waiting, click, click_through_driver, focused_of, git, host_then, install_twin,
    label, observed_label, open, open_surface, plugin_log, run_op, run_shell, shell, shows,
    value_of, wait_until,
};

#[gpui_kit::test]
fn a_generated_plugin_button_is_installed_found_clicked_and_checked_after_a_reload(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, data) = open_with_plugins_then(cx, |_| {});
    cx.run_until_parked();
    let folder = data.path().join("counter-lab");
    let id = PluginId("dev.kagan.counter-lab".into());

    // `kg plugin new` writes the starter, which has the panel and its KgButton.
    let created = run_op(
        cx,
        Op::Plugin(PluginOp::New {
            name: "counter-lab".into(),
            dir: Some(folder.clone()),
        }),
    );
    let OpResult::Files(files) = created else {
        panic!("the starter lists its files");
    };
    // The declarations it type-checks against include KgButton, so the starter can import it.
    let declared = files.iter().any(|file| {
        file.extension().is_some_and(|extension| extension == "ts")
            && std::fs::read_to_string(file).is_ok_and(|text| text.contains("KgButton"))
    });
    assert!(declared, "{files:?}");
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(folder.join("kagan.json")).unwrap()).unwrap();
    assert_eq!(manifest["commands"][0]["id"], "hello");
    assert!(manifest["commands"][0].get("key").is_none(), "{manifest}");

    // `kg plugin install` waits for the person. The test answers as the person would.
    let install = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source: folder.display().to_string(),
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("an approval waiting for the person")
            .id
            .clone()
    });
    let approved = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(approved).unwrap();
    cx.foreground_executor().block_test(install).unwrap();
    cx.run_until_parked();

    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
    open_surface(cx, &id, "hello");
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 0")
    });

    assert_eq!(click_through_driver(cx, window, "count", None), Ok(()));
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 1")
    });

    let reload = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Reload {
                id: id.clone(),
                thread_id: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(reload).unwrap();
    cx.run_until_parked();
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 0")
    });
    assert_eq!(click_through_driver(cx, window, "count", None), Ok(()));
    wait_until(cx, |cx| {
        observed_label(cx, window, "count").as_deref() == Some("Count 1")
    });

    // The starter's command runs from the shell, and writes to the log `kg plugin logs` reads.
    let ran = run_op(
        cx,
        Op::Plugin(PluginOp::Command {
            id: id.clone(),
            command: "hello".into(),
            thread_id: None,
            project_id: None,
        }),
    );
    assert_eq!(ran, OpResult::Done);
    let logs = run_op(
        cx,
        Op::Plugin(PluginOp::Logs {
            id,
            follow: false,
            after: None,
        }),
    );
    let OpResult::Logs { text, .. } = logs else {
        panic!("the logs of the starter")
    };
    assert!(text.contains("hello from the starter plugin"), "{text}");

    starter_key_runs_hello(cx, window);
}

/// The starter's `hello` has no key, so a second starter cannot lose one. Bound in Settings, the
/// key runs it from the window, with no command line.
pub(in crate::tests) fn starter_key_runs_hello(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let hellos = |cx: &mut TestAppContext| {
        plugin_log(cx, "dev.kagan.counter-lab")
            .matches("hello from the starter plugin")
            .count()
    };
    let row = "shortcut-plugin-dev.kagan.counter-lab-hello";
    click(cx, window, "settings-gear");
    click(cx, window, "setting-row-plugin-dev.kagan.counter-lab-hello");
    assert_eq!(observed_label(cx, window, row).as_deref(), Some("Not set"));
    click(cx, window, &format!("{row}-key"));
    cx.simulate_keystrokes(window, "secondary-shift-h");
    wait_until(cx, |cx| {
        observed_label(cx, window, row)
            .is_some_and(|label| label != "Not set" && label != "Press a key")
    });
    click(cx, window, "settings-close");
    let before = hellos(cx);
    cx.simulate_keystrokes(window, "secondary-shift-h");
    wait_until(cx, |cx| hellos(cx) > before);
}

/// Every observed element with this id, as its label, in the latest frame, sorted.
pub(in crate::tests) fn labels_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Vec<Option<String>> {
    let mut labels: Vec<Option<String>> = cx
        .update_window(window, |_, window, _| {
            crate::ui_registry::snapshots(window)
                .iter()
                .filter(|element| {
                    element.path.last() == Some(&ElementId::Name(name.to_owned().into()))
                })
                .map(|element| element.label.as_deref().map(str::to_owned))
                .collect()
        })
        .unwrap();
    labels.sort();
    labels
}

#[gpui_kit::test]
fn two_panels_with_one_local_id_are_ambiguous_until_a_scope_names_one(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_twin);
    cx.simulate_window_resize(window, size(px(1400.), px(1800.)));
    cx.run_until_parked();
    let open = |id: &str| {
        let task = cx.update(|cx| {
            Workspace::dispatch(
                Op::Surface(kg_domain::SurfaceOp::Open {
                    plugin: PluginId("dev.kagan.twin".into()),
                    id: id.into(),
                }),
                cx,
            )
        });
        cx.foreground_executor().block_test(task).unwrap();
    };
    open("a");
    open("b");
    wait_until(cx, |cx| labels_of(cx, window, "count").len() == 2);
    assert_eq!(
        labels_of(cx, window, "count"),
        [Some("A 0".into()), Some("B 0".into())]
    );

    // Without a scope the click is refused and nothing changes; it does not panic.
    let refused = click_through_driver(cx, window, "count", None);
    assert!(
        matches!(&refused, Err(OpError::Invalid(message)) if message.contains("2 elements")),
        "{refused:?}"
    );
    cx.run_until_parked();
    assert_eq!(
        labels_of(cx, window, "count"),
        [Some("A 0".into()), Some("B 0".into())]
    );

    // Scoped to panel B, the click reaches only that panel.
    let scoped = click_through_driver(cx, window, "count", Some("surface-page"));
    assert_eq!(scoped, Ok(()));
    wait_until(cx, |cx| {
        labels_of(cx, window, "count").contains(&Some("B 1".into()))
    });
    assert_eq!(
        labels_of(cx, window, "count"),
        [Some("A 0".into()), Some("B 1".into())]
    );
}

#[gpui_kit::test]
fn the_switch_and_the_custom_command_input_state_their_values(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    let value = |cx: &mut TestAppContext| {
        cx.update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name("agent-integration".into()))
                .and_then(|element| element.value.as_deref().map(str::to_owned))
        })
        .unwrap()
    };
    let before = value(cx);
    assert!(
        before == Some("on".into()) || before == Some("off".into()),
        "{before:?}"
    );
    click(cx, window, "agent-integration");
    cx.run_until_parked();
    assert_ne!(value(cx), before, "the switch reports its new value");
    click(cx, window, "settings-close");

    click(cx, window, "new-personal-thread");
    cx.run_until_parked();
    click(cx, window, "custom-command-toggle");
    click(cx, window, "custom-program");
    cx.simulate_input(window, "zsh");
    cx.run_until_parked();
    let program = cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name("custom-program".into()))
            .and_then(|element| element.value.as_deref().map(str::to_owned))
    });
    assert_eq!(program.unwrap(), Some("zsh".into()));
}

#[gpui_kit::test]
fn installing_from_the_plugins_box_asks_for_approval_in_needs_you(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, false);
    // A small plugin whose clipboard power needs the person's approval.
    let plugin = tempfile::tempdir().unwrap();
    let manifest = serde_json::json!({
        "id": "dev.kagan.echo",
        "name": "Echo",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Echo things back.",
        "powers": { "clipboard": { "write": true } },
        "ui": { "panels": [] }
    });
    std::fs::write(plugin.path().join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        plugin.path().join("main.js"),
        "export default class Plugin {}\n",
    )
    .unwrap();
    let root = dunce::canonicalize(plugin.path()).unwrap();

    // Type the folder into the install form on the Installed page and press its Install button.
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-install-open");
    click(cx, window, "plugin-source");
    cx.simulate_input(window, &root.display().to_string());
    cx.run_until_parked();
    click(cx, window, "plugin-install");
    cx.run_until_parked();

    let approval = cx
        .update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .approvals()
                .next()
                .map(|approval| approval.id.clone())
        })
        .expect("an approval waiting for the person");
    assert!(shows(cx, window, &format!("answer-{}-decline", approval.0)));
    assert_eq!(
        label(cx, window, &format!("answer-{}-approve", approval.0)),
        "Install"
    );
    assert_eq!(
        label(cx, window, &format!("answer-{}-decline", approval.0)),
        "Cancel"
    );

    assert!(!shows(cx, window, "plugin-install-open"));
    assert!(!shows(cx, window, "plugin-install-form"));
    assert!(!shows(cx, window, "plugin-install"));
    assert!(!shows(cx, window, "plugin-source"));
    click(cx, window, "setting-row-install-waiting");
    assert!(shows(cx, window, "setting-row-install-waiting"));

    // Only the person answers an install; the test answers through the same core call.
    let accepted =
        cx.update(|cx| Workspace::answer_standalone(approval.clone(), "approve".into(), cx));
    cx.foreground_executor().block_test(accepted).unwrap();
    wait_until(cx, |cx| shows(cx, window, "plugin-row-dev.kagan.echo"));
    assert!(!shows(cx, window, "plugin-install-form"));
    assert_eq!(
        focused_of(cx, window, "plugin-row-dev.kagan.echo"),
        Some(true)
    );
    click(cx, window, "plugin-install-open");
    assert_eq!(value_of(cx, window, "plugin-source"), Some(String::new()));
    assert!(!shows(
        cx,
        window,
        &format!("answer-{}-decline", approval.0)
    ));
}

#[gpui_kit::test]
fn choosing_a_plugin_folder_fills_the_install_form(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open_with_plugins(cx, false);
    click(cx, window, "settings-gear");
    click(cx, window, "plugin-install-open");
    click(cx, window, "plugin-choose-folder");
    assert!(cx.did_prompt_for_paths());
    let path = data.path().to_path_buf();
    let expected = path.to_string_lossy().into_owned();
    cx.simulate_path_prompt_response(move |_| Some(vec![path]));
    cx.run_until_parked();
    assert_eq!(value_of(cx, window, "plugin-source"), Some(expected));
}

/// The open diff's lines as the window holds them.
pub(in crate::tests) fn open_lines(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Vec<String> {
    cx.update(|cx| review_read::open_diff_lines(kagan.read(cx)))
}

/// A git repo with one commit, added as a project. Returns its folder and the project.
pub(in crate::tests) fn committed_repo(
    cx: &mut TestAppContext,
    data: &Path,
) -> (std::path::PathBuf, Project) {
    let repo = init_repo(data);
    let project = add_project(cx, &repo);
    (repo, project)
}

/// A git folder with one commit, not yet a project.
pub(in crate::tests) fn init_repo(data: &Path) -> std::path::PathBuf {
    let repo = data.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    std::fs::write(repo.join("notes.txt"), "one\n").unwrap();
    git(&repo, &["add", "."]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "start",
        ],
    );
    repo
}

pub(in crate::tests) fn add_project(cx: &mut TestAppContext, path: &Path) -> Project {
    let added = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Add {
                path: path.to_path_buf(),
            }),
            cx,
        )
    });
    let OpResult::Project(project) = cx.foreground_executor().block_test(added).unwrap() else {
        panic!("project");
    };
    project
}

/// A git repo with one committed file, added as a project, and a thread running in it. The file
/// is changed on disk after the commit, and the thread is shown.
pub(in crate::tests) fn repo_thread(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    data: &Path,
) -> (ThreadId, std::path::PathBuf) {
    let (repo, project) = committed_repo(cx, data);
    std::fs::write(repo.join("notes.txt"), "one\ntwo\n").unwrap();
    let run = Run {
        project_id: project.id,
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::ProjectDefault,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(started).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    click(cx, window, &format!("thread-{}", thread.id.0));
    (thread.id, repo)
}

/// A running shell thread that no sidebar row lists.
pub(in crate::tests) fn run_hidden_shell(cx: &mut TestAppContext) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Hidden),
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let task = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    thread.id
}

#[gpui_kit::test]
fn a_plugin_panel_shows_beside_the_built_in_panels(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins(cx, true);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    cx.run_until_parked();
    assert!(shows(cx, window, "needs-you"));
    assert!(shows(cx, window, "changes"));
    assert!(!shows(cx, window, "plugins"));
    assert!(shows(cx, window, "plugins-manage"));
    assert!(!shows(cx, window, PAGE_FRAME));
}

pub(in crate::tests) const PAGE: &str = "dev.test.page";
pub(in crate::tests) const PAGE_FRAME: &str = "panel-dev.test.page-page";

/// Writes a plugin with one center page that follows the thread, installed and enabled.
pub(in crate::tests) fn install_page(data: &Path) {
    let root = data.join("page");
    std::fs::create_dir(&root).unwrap();
    let manifest = serde_json::json!({
        "id": PAGE,
        "name": "Page",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A test page.",
        "powers": {},
        "ui": {"panels": [{"id": "page", "title": "Page", "view": "Page", "placement": "center", "follows": "thread"}]}
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
export default class Plugin extends View {}
export class Page extends View {
  render() { return div().child(new KgButton("page-button").label("Page")); }
}
"#,
    )
    .unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(PAGE.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

/// Opens the window with the plugin host running. `page` also installs a plugin with a center page.
pub(in crate::tests) fn open_with_plugins(
    cx: &mut TestAppContext,
    page: bool,
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    open_with_plugins_then(cx, |data| {
        if page {
            install_page(data);
        }
    })
}

/// Opens the window with the plugin host running, after `install` has written the plugins.
pub(in crate::tests) fn open_with_plugins_then(
    cx: &mut TestAppContext,
    install: impl FnOnce(&Path),
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let data = host_then(cx, install);
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| kagan.read(cx).window_handle());
    (kagan, window, data)
}

pub(in crate::tests) fn host(cx: &mut TestAppContext) -> tempfile::TempDir {
    host_then(cx, |_| {})
}
