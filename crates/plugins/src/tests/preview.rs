use std::path::{Path, PathBuf};
#[cfg(unix)]
use std::process::Command;
use std::process::Stdio;
use std::time::Duration;
#[cfg(unix)]
use std::time::Instant;

use gpui_kit::{Entity, Task, TestAppContext};
use kg_core::{previews_dir, seed_preview};
#[cfg(unix)]
use kg_domain::{Op, ThreadOp};
use kg_domain::{
    OpError, OpResult, Plugin, PluginId, PluginOp, PluginPreview, PluginPreviewOp, PreviewId,
    ThreadId, prefs,
};
use kg_process::Process;
use kg_store::Store;
use serde_json::json;

use crate::PluginHost;
use crate::host::Hold;
use crate::manifest::Grant;
use crate::runtime::preview::{self, Running};
use kg_core::NO_ONE_HERE;

#[cfg(unix)]
use super::alive;
#[cfg(unix)]
use super::run;
use super::{
    ID, PluginHostGlobal, answer, call, hold_at, open_kept, plugin_dir, plugin_loaded, plugin_op,
    read_manifest, records, sample_manifest, sleep_command, thread, write_plugin,
};

fn start(path: &Path) -> PluginOp {
    PluginOp::Preview {
        op: PluginPreviewOp::Start {
            path: path.to_path_buf(),
            thread_id: Some(thread()),
        },
    }
}

fn preview_folders(data: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(previews_dir(data))
        .map(|entries| {
            entries
                .map(|entry| entry.unwrap().path())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn host_of(cx: &mut TestAppContext) -> Entity<PluginHost> {
    cx.read(|cx| cx.global::<PluginHostGlobal>().0.clone())
}

// Waits five seconds of test time. Gives None when the task has not finished by then.
fn within_five_seconds<T: 'static>(cx: &mut TestAppContext, task: Task<T>) -> Option<T> {
    let timeout = cx.executor().timer(Duration::from_secs(5));
    cx.foreground_executor().block_test(async move {
        match futures::future::select(Box::pin(task), Box::pin(timeout)).await {
            futures::future::Either::Left((value, _)) => Some(value),
            futures::future::Either::Right(_) => None,
        }
    })
}

// A tool call loads the plugin, the way a person's agent would reach it.
fn echo(cx: &mut TestAppContext) -> Result<OpResult, OpError> {
    call(cx, "echo", json!({ "text": "hi" }))
}

fn preview_at(folder: &Path, id: &str, thread: Option<ThreadId>, plugin: &str) -> PluginPreview {
    PluginPreview {
        id: PreviewId(id.into()),
        plugin: PluginId(plugin.into()),
        name: "Echo".into(),
        thread_id: thread,
        path: folder.to_path_buf(),
        data_dir: folder.join("data"),
        socket: folder.join("kg.sock"),
    }
}

// A running preview whose app and watch are sleeping processes, so a test can see them end.
fn sleeping(folder: &Path, info: PluginPreview) -> Running {
    let app = Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let watch =
        Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    Running::new(info, folder.to_path_buf(), app, watch)
}

// A sleeping app with a known pid, so a test can see that it ends.
#[cfg(unix)]
fn sleeping_app() -> (Process, u32) {
    let mut app =
        Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let pid = app.child().id();
    (app, pid)
}

#[cfg(unix)]
fn fake_kg(dir: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join("fake-kg");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[gpui_kit::test]
fn declining_a_preview_writes_nothing_and_starts_nothing(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let started = plugin_op(cx, start(plugin.path()));
    answer(cx, "decline");

    assert!(cx.foreground_executor().block_test(started).is_err());
    assert!(
        preview_folders(&path).is_empty(),
        "a declined preview left a folder"
    );
}

#[gpui_kit::test]
fn approving_a_preview_seeds_its_folder_with_the_approved_grant(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let root = dunce::canonicalize(plugin.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let release = hold_at(cx, Hold::PreviewStart);

    let _started = plugin_op(cx, start(&root));
    answer(cx, "approve");

    let folders = preview_folders(&path);
    let [folder] = folders.as_slice() else {
        panic!("one preview folder: {folders:?}");
    };
    let store = Store::open(folder.join("data").join("kagan.sqlite3")).unwrap();
    let grant = Grant::of(&read_manifest(&root).unwrap(), &root);
    let stored = store.plugins().unwrap();
    assert_eq!(stored.len(), 1);
    assert_eq!(stored[0].id, PluginId(ID.into()));
    assert_eq!(stored[0].approved, serde_json::to_value(&grant).unwrap());
    assert_eq!(store.setting(prefs::PREVIEW).unwrap(), Some(json!(true)));
    drop(release);
}

#[gpui_kit::test]
fn a_failed_start_removes_its_folder_and_says_so(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let started = plugin_op(cx, start(plugin.path()));
    answer(cx, "approve");

    assert!(cx.foreground_executor().block_test(started).is_err());
    assert!(
        preview_folders(&path).is_empty(),
        "a failed start left its folder"
    );
}

#[gpui_kit::test]
fn a_second_start_does_not_sweep_a_folder_still_being_made(cx: &mut TestAppContext) {
    let first = plugin_dir();
    let mut other = sample_manifest();
    other["id"] = json!("dev.test.other");
    let second = tempfile::tempdir().unwrap();
    write_plugin(second.path(), &other);
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let held = hold_at(cx, Hold::PreviewStart);
    let _first = plugin_op(cx, start(first.path()));
    answer(cx, "approve");
    cx.run_until_parked();
    assert_eq!(
        preview_folders(&path).len(),
        1,
        "the first start did not reach its hold"
    );

    let held_again = hold_at(cx, Hold::PreviewStart);
    let _second = plugin_op(cx, start(second.path()));
    answer(cx, "approve");
    cx.run_until_parked();

    assert_eq!(
        preview_folders(&path).len(),
        2,
        "the second start's sweep removed the first start's folder"
    );
    drop((held, held_again));
}

#[gpui_kit::test]
fn a_sweep_removes_leftovers_and_keeps_running_ids_and_other_names(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let root = previews_dir(&path);
    for name in ["p-aaaaaa", "p-bbbbbb", "p-cccccc", "other", "p-zz"] {
        std::fs::create_dir_all(root.join(name)).unwrap();
    }
    let keep = [PreviewId("p-bbbbbb".into())];

    preview::sweep(Path::new("/nonexistent/kagan-test-kg"), &path, &keep).unwrap();

    let mut left: Vec<String> = preview_folders(&path)
        .iter()
        .map(|folder| folder.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    left.sort();
    assert_eq!(left, ["other", "p-bbbbbb", "p-zz"]);
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_sweep_keeps_a_folder_whose_app_still_answers_after_a_quit(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let folder = previews_dir(&path).join("p-dddddd");
    std::fs::create_dir_all(&folder).unwrap();
    let exe = fake_kg(&path, "exit 0");

    preview::sweep(&exe, &path, &[]).unwrap();

    assert!(
        folder.exists(),
        "a folder whose app still answers was removed"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn stop_ends_the_preview_and_its_app(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let folder = previews_dir(&path).join("p-eeeeee");
    std::fs::create_dir_all(&folder).unwrap();
    let (app, pid) = sleeping_app();
    let watch =
        Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let info = preview_at(&folder, "p-eeeeee", Some(thread()), ID);
    let host = host_of(cx);
    host.update(cx, |host, _| {
        host.previews
            .replace(Running::new(info, folder.clone(), app, watch));
    });

    let stopped = plugin_op(
        cx,
        PluginOp::Preview {
            op: PluginPreviewOp::Stop {
                id: PreviewId("p-eeeeee".into()),
            },
        },
    );
    cx.run_until_parked();
    assert!(
        within_five_seconds(cx, stopped)
            .expect("stop did not finish")
            .is_ok()
    );

    assert!(!folder.exists(), "stop left the folder");
    assert!(
        !alive(&pid.to_string()),
        "the preview's app is still running"
    );
}

#[gpui_kit::test]
fn stop_of_an_unknown_id_says_what_is_running(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let stopped = plugin_op(
        cx,
        PluginOp::Preview {
            op: PluginPreviewOp::Stop {
                id: PreviewId("p-ffffff".into()),
            },
        },
    );
    let message = match within_five_seconds(cx, stopped).expect("stop did not finish") {
        Err(OpError::NotFound(message)) => message,
        other => panic!("an unknown preview was not reported: {other:?}"),
    };
    assert_eq!(message, "no preview p-ffffff; none is running");
}

// A second preview for the same thread and plugin replaces the first, and the first comes back.
#[gpui_kit::test]
fn replacing_a_thread_preview_returns_the_one_it_replaces(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let first = sleeping(&path, preview_at(&path, "p-000001", Some(thread()), ID));
    let second = sleeping(&path, preview_at(&path, "p-000002", Some(thread()), ID));
    let (first_back, kept) = host_of(cx).update(cx, |host, _| {
        let none = host.previews.replace(first);
        let back = host.previews.replace(second);
        (
            none.is_none(),
            (back.map(|old| old.info.id), host.previews.infos()),
        )
    });
    assert!(first_back, "the first preview replaced something");
    let (replaced, infos) = kept;
    assert_eq!(replaced, Some(PreviewId("p-000001".into())));
    let ids: Vec<PreviewId> = infos.into_iter().map(|info| info.id).collect();
    assert_eq!(ids, [PreviewId("p-000002".into())]);
}

#[cfg(unix)]
#[gpui_kit::test]
fn quitting_the_person_app_ends_each_watch_at_once_and_asks_each_preview_to_quit(
    cx: &mut TestAppContext,
) {
    cx.executor().allow_parking();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let folder = previews_dir(&path).join("p-777777");
    std::fs::create_dir_all(&folder).unwrap();
    let asked = path.join("asked.log");
    let exe = fake_kg(
        &path,
        &format!("printf '%s\\n' \"$*\" >> '{}'", asked.display()),
    );
    let (app, _app_pid) = sleeping_app();
    let mut watch =
        Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let watch_pid = watch.child().id();
    let info = preview_at(&folder, "p-777777", Some(thread()), ID);
    host_of(cx).update(cx, |host, _| {
        host.previews.exe = exe.clone();
        host.previews
            .replace(Running::new(info, folder.clone(), app, watch));
    });
    // The app's own quit runs the handler that ends the previews. It must not wait for them: a
    // preview's app is given three seconds to exit, and the quit has to return well before that.
    let quitting = Instant::now();
    cx.quit();
    assert!(
        quitting.elapsed() < Duration::from_secs(2),
        "the quit waited {:?} for the previews",
        quitting.elapsed()
    );

    std::thread::sleep(Duration::from_millis(100));
    // A killed child is a zombie until it is reaped; a zombie has ended, so it is not running.
    let state = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &watch_pid.to_string()])
        .output()
        .unwrap();
    let state = String::from_utf8_lossy(&state.stdout).trim().to_owned();
    assert!(
        state.is_empty() || state.starts_with('Z'),
        "the watch outlived the quit: {state}"
    );
    let started = Instant::now();
    while !std::fs::read_to_string(&asked)
        .unwrap_or_default()
        .contains("window quit")
    {
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "the preview was never asked to quit"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    host_of(cx).update(cx, |host, _| {
        host.previews.remove(&PreviewId("p-777777".into()));
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn archiving_a_thread_ends_its_previews(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let folder = previews_dir(&path).join("p-111111");
    std::fs::create_dir_all(&folder).unwrap();
    let (app, pid) = sleeping_app();
    let watch =
        Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let info = preview_at(&folder, "p-111111", Some(thread()), ID);
    host_of(cx).update(cx, |host, _| {
        host.previews
            .replace(Running::new(info, folder.clone(), app, watch));
    });

    run(cx, Op::Thread(ThreadOp::Archive { id: thread() })).unwrap();
    cx.run_until_parked();

    let left = host_of(cx).read_with(cx, |host, _| host.previews.infos());
    assert!(
        left.is_empty(),
        "an archived thread kept its preview: {left:?}"
    );
    assert!(!folder.exists(), "archiving left the folder");
    assert!(
        !alive(&pid.to_string()),
        "the preview's app outlived its thread"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn list_shows_only_previews_that_still_run(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let folder = previews_dir(&path).join("p-222222");
    std::fs::create_dir_all(&folder).unwrap();
    let mut command = Command::new("sh");
    command.args(["-c", "exit 0"]);
    let mut app = Process::spawn(command, Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    app.wait_within(Duration::from_secs(5), || false).unwrap();
    let watch =
        Process::spawn(sleep_command(), Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let info = preview_at(&folder, "p-222222", Some(thread()), ID);
    host_of(cx).update(cx, |host, _| {
        host.previews
            .replace(Running::new(info, folder.clone(), app, watch));
    });

    let listed = run(
        cx,
        Op::Plugin(PluginOp::Preview {
            op: PluginPreviewOp::List,
        }),
    )
    .unwrap();
    assert!(
        matches!(listed, OpResult::PluginPreviews(ref list) if list.is_empty()),
        "{listed:?}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_preview_quit_is_asked_over_its_socket_without_the_prompt(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    let folder = previews_dir(&path).join("p-333333");
    std::fs::create_dir_all(&folder).unwrap();
    let log = path.join("asked.log");
    let exe = fake_kg(
        &path,
        &format!(
            "printf '%s|%s|%s\\n' \"$*\" \"$KG_NO_START\" \"$KG_SOCKET\" >> '{}'",
            log.display()
        ),
    );
    let info = preview_at(&folder, "p-333333", Some(thread()), ID);
    let running = sleeping(&folder, info.clone());

    preview::shut_down(running, &exe).unwrap();

    let asked = std::fs::read_to_string(&log).expect("the preview was never asked to quit");
    assert_eq!(
        asked.trim(),
        format!("window quit|1|{}", info.socket.display())
    );
    assert!(!folder.exists());
}

#[cfg(unix)]
#[test]
fn a_preview_app_that_exits_at_once_fails_the_wait_with_its_log() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("app.log"),
        "boom: the window would not open\n",
    )
    .unwrap();
    let mut command = Command::new("sh");
    command.args(["-c", "exit 3"]);
    let mut app_exiting =
        Process::spawn(command, Stdio::null(), Stdio::null(), Stdio::null()).unwrap();
    let started = Instant::now();

    let error = preview::wait_ready(
        Path::new("/nonexistent/kagan-test-kg"),
        &dir.path().join("data"),
        &dir.path().join("kg.sock"),
        dir.path(),
        &mut app_exiting,
    )
    .unwrap_err();

    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the wait ran on after the app exited"
    );
    let OpError::Failed(message) = error else {
        panic!("{error:?}");
    };
    assert!(message.contains("stopped before it answered"), "{message}");
    assert!(
        message.contains("boom: the window would not open"),
        "{message}"
    );
}

#[cfg(unix)]
#[test]
fn a_socket_path_longer_than_the_platform_limit_is_refused() {
    let folder = PathBuf::from(format!("/{}", "a".repeat(120)));
    assert!(matches!(
        preview::check_socket_length(&folder.join("kg.sock")),
        Err(OpError::Invalid(message)) if message.contains("socket path longer")
    ));
}

#[gpui_kit::test]
fn a_preview_refuses_a_question_at_once(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let root = dunce::canonicalize(plugin.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    seed_preview(&path, &installed_record(&root)).unwrap();
    open_kept(cx, &path, data, &[]);

    let removed = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: Some(thread()),
        },
    );
    let outcome = within_five_seconds(cx, removed);

    let Some(Err(OpError::Invalid(message))) = outcome else {
        panic!("a question in a preview did not refuse at once: {outcome:?}");
    };
    assert!(message.contains("no one can answer here"), "{message}");
}

#[gpui_kit::test]
fn preview_start_and_stop_refuse_inside_a_preview(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let root = dunce::canonicalize(plugin.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    seed_preview(&path, &installed_record(&root)).unwrap();
    open_kept(cx, &path, data, &[]);

    for op in [
        start(&root),
        PluginOp::Preview {
            op: PluginPreviewOp::Stop {
                id: PreviewId("p-555555".into()),
            },
        },
    ] {
        let task = plugin_op(cx, op);
        let outcome = within_five_seconds(cx, task);
        let Some(Err(OpError::Invalid(message))) = outcome else {
            panic!("a preview op inside a preview was not refused: {outcome:?}");
        };
        assert_eq!(message, NO_ONE_HERE);
    }
}

// The record a preview folder starts with: the plugin, approved as its manifest asks today.
fn installed_record(root: &Path) -> Plugin {
    Plugin {
        id: PluginId(ID.into()),
        source: root.display().to_string(),
        approved: serde_json::to_value(Grant::of(&read_manifest(root).unwrap(), root)).unwrap(),
        root: root.to_path_buf(),
        commit: None,
        enabled: true,
        skills: Vec::new(),
        error: None,
    }
}

#[gpui_kit::test]
fn a_preview_refuses_a_reload_whose_powers_grew_and_keeps_the_loaded_code(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let root = dunce::canonicalize(plugin.path()).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    let record = installed_record(&root);
    seed_preview(&path, &record).unwrap();
    open_kept(cx, &path, data, &[]);
    assert!(matches!(
        echo(cx),
        Ok(OpResult::ToolReply {
            is_error: false,
            ..
        })
    ));
    assert!(plugin_loaded(cx, ID), "the approved plugin did not load");

    let mut grown = sample_manifest();
    grown["powers"] = json!({ "clipboard": { "write": true }, "calls": ["dev.test.other"] });
    write_plugin(&root, &grown);

    let reloaded = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    let outcome = within_five_seconds(cx, reloaded);
    let Some(Err(OpError::Invalid(message))) = outcome else {
        panic!("a preview reload with grown powers was not refused: {outcome:?}");
    };
    assert!(
        message.contains("run kg plugin preview start again so the person can approve them"),
        "{message}"
    );
    let kept = records(cx);
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].approved, record.approved, "the old grant changed");
    assert!(plugin_loaded(cx, ID), "the approved code stopped running");
    assert!(matches!(
        echo(cx),
        Ok(OpResult::ToolReply {
            is_error: false,
            ..
        })
    ));
}
