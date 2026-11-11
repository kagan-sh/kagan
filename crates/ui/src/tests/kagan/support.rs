use crate::kagan::screen_read;
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use kg_core::{Grid, Workspace};
use kg_domain::{
    EnvironmentSpec, Op, OpResult, PreferenceOp, Project, ProjectId, ProjectKind, Run, ThreadId,
    ThreadOp, ThreadStatus, Visibility,
};

use crate::kagan::Kagan;

use super::shell;
use crate::terminal::TerminalView;

pub(in crate::tests) fn host_then(
    cx: &mut TestAppContext,
    install: impl FnOnce(&Path),
) -> tempfile::TempDir {
    let data = app_then(cx, install);
    let started = cx.update(|cx| {
        let mut catalog = gpui_shell::components::registry().unwrap();
        crate::register_components(&mut catalog).unwrap();
        kg_plugins::init(cx, catalog.freeze().unwrap())
    });
    cx.foreground_executor().block_test(started).unwrap();
    data
}

/// The workspace and app, as `host_then` builds them, but with no plugin host.
pub(in crate::tests) fn app_then(
    cx: &mut TestAppContext,
    install: impl FnOnce(&Path),
) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    seed(data.path());
    install(data.path());
    let opened = cx.update(|cx| {
        Workspace::open(
            data.path().into(),
            std::env::current_exe().unwrap(),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_shell::init(cx);
        crate::init(cx);
    });
    data
}

pub(in crate::tests) fn git(path: &Path, args: &[&str]) {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args([
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

pub(in crate::tests) fn pending(
    cx: &mut TestAppContext,
    id: &ThreadId,
) -> Vec<kg_domain::PendingInteraction> {
    let task = cx.update(|cx| {
        Workspace::dispatch(Op::Thread(ThreadOp::Interactions { id: id.clone() }), cx)
    });
    let OpResult::Interactions(list) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("interactions");
    };
    list
}

pub(crate) fn open(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let data = setup(cx);
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    (kagan, window, data)
}

/// A window on a data folder with no project, thread or setting yet: a fresh install.
pub(crate) fn open_empty(
    cx: &mut TestAppContext,
) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let data = setup_with(cx, false);
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    (kagan, window, data)
}

pub(in crate::tests) fn setup(cx: &mut TestAppContext) -> tempfile::TempDir {
    setup_with(cx, true)
}

pub(in crate::tests) fn setup_with(cx: &mut TestAppContext, seeded: bool) -> tempfile::TempDir {
    let data = tempfile::tempdir().unwrap();
    if seeded {
        seed(data.path());
    }
    let opened = cx.update(|cx| {
        Workspace::open(
            data.path().into(),
            std::env::current_exe().unwrap(),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
    // No test finds a program in the machine's own install folders.
    let no_home = data.path().join("no-home");
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_home(&no_home);
        });
    });
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::init(cx);
    });
    data
}

pub(in crate::tests) fn seed(dir: &Path) {
    let store = kg_store::Store::open(dir.join("kagan.sqlite3")).unwrap();
    store
        .put_project(&Project {
            id: ProjectId("prj_kagan".into()),
            kind: ProjectKind::Personal,
            name: "Kagan project".into(),
            path: dir.to_path_buf(),
            git_remote_url: None,
            sort: 0,
            program: None,
        })
        .unwrap();
}

pub(crate) fn run_shell(cx: &mut TestAppContext) -> ThreadId {
    run_program(cx, shell())
}

pub(in crate::tests) fn run_program(cx: &mut TestAppContext, program: &str) -> ThreadId {
    run_titled(cx, program, None)
}

// A run with a title given, or none so the program names the thread.
pub(in crate::tests) fn run_titled(
    cx: &mut TestAppContext,
    program: &str,
    title: Option<&str>,
) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: program.into(),
        title: title.map(str::to_owned),
        environment: EnvironmentSpec::Personal,
        visibility: None,
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

/// A thread run under `parent`, shown in the sidebar under it.
pub(in crate::tests) fn run_child(cx: &mut TestAppContext, parent: &ThreadId) -> ThreadId {
    let run = Run {
        project_id: ProjectId("prj_kagan".into()),
        program: shell().into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: Some(Visibility::Visible),
        parent_thread_id: Some(parent.clone()),
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

pub(in crate::tests) fn input(id: &ThreadId, text: &str) -> Op {
    Op::Thread(ThreadOp::Input {
        id: id.clone(),
        text: text.into(),
        terminal: None,
    })
}

/// Writes one preference through the same Op `kg preference set` runs.
pub(in crate::tests) fn set_preference(
    cx: &mut TestAppContext,
    key: &str,
    value: serde_json::Value,
) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: key.into(),
                value,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn click_within(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
    within: &str,
) {
    cx.update_window(window, |_, window, cx| {
        let id = ElementId::Name(name.to_owned().into());
        let scope = ElementId::Name(within.to_owned().into());
        crate::ui_dispatch::click_within(window, &id, Some(&scope), cx)
            .unwrap_or_else(|error| panic!("{name} in {within}: {error}"));
    })
    .unwrap();
    cx.run_until_parked();
}

pub(in crate::tests) fn click(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    cx.update_window(window, |_, window, cx| {
        let id = ElementId::Name(name.to_owned().into());
        crate::ui_dispatch::click(window, &id, cx)
            .unwrap_or_else(|error| panic!("{name}: {error}"));
    })
    .unwrap();
    cx.run_until_parked();
}

/// Counts how often the view asks to be drawn. Keep the subscription alive as long as the count.
pub(in crate::tests) fn count_repaints(
    cx: &mut TestAppContext,
    view: &Entity<TerminalView>,
) -> (Rc<Cell<usize>>, gpui_kit::Subscription) {
    let count = Rc::new(Cell::new(0));
    let seen = count.clone();
    let subscription = cx.update(|cx| cx.observe(view, move |_, _| seen.set(seen.get() + 1)));
    (count, subscription)
}

pub(in crate::tests) fn host_grid(cx: &mut TestAppContext, id: &ThreadId) -> Grid {
    cx.update(|cx| Workspace::global(cx).read(cx).grid(id, None))
        .expect("the host holds a screen")
}

pub(in crate::tests) fn grid_text(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> String {
    let grid = cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx));
    grid.map_or_else(String::new, |grid| {
        grid.lines
            .iter()
            .map(|line| line.iter().map(|cell| cell.c).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    })
}

pub(in crate::tests) fn thread_record(cx: &mut TestAppContext, id: &ThreadId) -> kg_domain::Thread {
    let task =
        cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Get { id: id.clone() }), cx));
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Thread(thread) => thread,
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn thread_status(cx: &mut TestAppContext, id: &ThreadId) -> ThreadStatus {
    thread_record(cx, id).status
}

#[track_caller]
pub(in crate::tests) fn wait_until(
    cx: &mut TestAppContext,
    mut done: impl FnMut(&mut TestAppContext) -> bool,
) {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "timed out waiting for the screen"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// The shell prints in real time, which the test clock cannot speed up, so this wait gets no short
// limit. The 120 s limit only stops a test that can never pass from hanging.
#[cfg(unix)]
pub(in crate::tests) fn wait_for_shell(
    cx: &mut TestAppContext,
    mut done: impl FnMut(&mut TestAppContext) -> bool,
) {
    let hang = Instant::now() + Duration::from_secs(120);
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        assert!(
            Instant::now() < hang,
            "the shell never printed what the test waits for"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}

// In a short window the controls below the fields sit below the fold, so a person scrolls the form to them.
pub(in crate::tests) fn scroll_form_to_end(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::scroll_within(
            window,
            &ElementId::Name("start-fields".into()),
            None,
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(-1000.))),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    cx.run_until_parked();
}
