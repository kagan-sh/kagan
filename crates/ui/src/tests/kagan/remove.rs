#[cfg(unix)]
use std::path::Path;
#[cfg(unix)]
use std::time::Duration;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
use gpui_kit::{
    InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point,
};
use kg_core::Workspace;
use kg_domain::Op;
#[cfg(unix)]
use kg_domain::{EnvironmentStatus, ThreadStatus};

#[cfg(unix)]
use crate::kagan::{ArchiveThread, StopThread};

use super::{add_project, approvals_waiting, click, open, send, shows, wait_until};
#[cfg(unix)]
use super::{
    commit_setup, committed_repo, environment_of, label, scroll_form_to_end, set_program, shell,
    shown, thread_record,
};

#[cfg(unix)]
#[gpui_kit::test]
fn a_blocked_removal_is_shown_and_can_be_tried_again(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "touch kept-marker\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    scroll_form_to_end(cx, window);
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let thread = shown(cx, &kagan).unwrap();
    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| {
        thread_record(cx, &thread).status == ThreadStatus::Idle
    });
    cx.dispatch_action(window, ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &thread).archived_at.is_some());
    let mut environment = environment_of(cx, &thread);
    for _ in 0..40 {
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        environment = environment_of(cx, &thread);
        if environment.status == EnvironmentStatus::Error {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let reason = environment.error.as_deref().unwrap_or("");
    assert!(reason.contains("kept-marker"), "{reason}");
    assert!(reason.contains("git-ignore"), "{reason}");
    click(cx, window, "archived");
    let removal = format!("removal-{}", thread.0);
    wait_until(cx, |cx| label(cx, window, &removal).contains("kept-marker"));
    assert!(label(cx, window, &removal).contains("remove the worktree again"));
    std::fs::remove_file(environment.path.join("kept-marker")).unwrap();
    click(cx, window, &format!("remove-again-{}", environment.id.0));
    for _ in 0..40 {
        cx.run_until_parked();
        environment = environment_of(cx, &thread);
        if !environment.path.exists() {
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!environment.path.exists(), "{environment:?}");
    assert!(environment.error.is_none(), "{environment:?}");
    cx.run_until_parked();
    assert!(!shows(cx, window, &removal));
}

#[cfg(unix)]
pub(in crate::tests) fn worktree_dirs(data: &Path) -> Vec<std::path::PathBuf> {
    let Ok(read) = std::fs::read_dir(data.join("worktrees")) else {
        return Vec::new();
    };
    let mut paths: Vec<_> = read
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| std::fs::read_dir(entry.path()).ok())
        .flat_map(std::iter::Iterator::flatten)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    paths.sort();
    paths
}

#[cfg(unix)]
pub(in crate::tests) fn only_worktree(data: &Path) -> std::path::PathBuf {
    let paths = worktree_dirs(data);
    assert_eq!(paths.len(), 1, "{paths:?}");
    paths.into_iter().next().unwrap()
}

pub(in crate::tests) fn right_click(cx: &mut TestAppContext, window: AnyWindowHandle, row: &str) {
    let at = cx
        .update_window(window, |_, window, _| {
            let bounds = crate::ui_registry::find(window, &ElementId::Name(row.to_owned().into()))
                .unwrap()
                .bounds;
            point(
                bounds.origin.x + bounds.size.width / 2.,
                bounds.origin.y + bounds.size.height / 2.,
            )
        })
        .unwrap();
    send(
        cx,
        window,
        MouseMoveEvent {
            position: at,
            pressed_button: None,
            modifiers: Modifiers::default(),
        }
        .to_platform_input(),
    );
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
    send(
        cx,
        window,
        MouseDownEvent {
            button: MouseButton::Right,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
    );
    send(
        cx,
        window,
        MouseUpEvent {
            button: MouseButton::Right,
            position: at,
            modifiers: Modifiers::default(),
            click_count: 1,
        }
        .to_platform_input(),
    );
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn project_menu_asks_and_removal_updates_the_sidebar(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("project-to-remove");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    let row = format!("project-{}", project.id);
    wait_until(cx, |cx| shows(cx, window, &row));
    right_click(cx, window, &row);
    wait_until(cx, |cx| shows(cx, window, "project-remove"));
    click(cx, window, "project-remove");
    wait_until(cx, |cx| approvals_waiting(cx) == 1);
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    let cancelled = cx.update(|cx| Workspace::answer_standalone(approval, "decline".into(), cx));
    cx.foreground_executor().block_test(cancelled).unwrap();
    wait_until(cx, |cx| approvals_waiting(cx) == 0);
    assert!(shows(cx, window, &row));
    let confirmed = cx.update(|cx| {
        Workspace::dispatch(
            Op::Project(kg_domain::ProjectOp::Remove {
                id: project.id.clone(),
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
            .unwrap()
            .id
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(confirmed).unwrap();
    wait_until(cx, |cx| !shows(cx, window, &row));
    assert!(folder.exists());
}

#[gpui_kit::test]
fn the_personal_project_has_no_remove_menu(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("standard-project");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    let row = format!("project-{}", project.id);
    wait_until(cx, |cx| shows(cx, window, &row));
    right_click(cx, window, "project-personal");
    cx.run_until_parked();
    assert!(!shows(cx, window, "project-remove"));
    right_click(cx, window, &row);
    wait_until(cx, |cx| shows(cx, window, "project-remove"));
}
