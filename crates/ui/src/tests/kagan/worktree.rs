use super::chord;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentOp, EnvironmentProvider, EnvironmentStatus, Op, OpResult, ProjectId, ThreadId,
    ThreadOp,
};

use crate::kagan::Kagan;

use super::{
    add_project, click, committed_repo, git, init_repo, label, open, shell, shows, thread_record,
    wait_until,
};

#[gpui_kit::test]
fn a_second_start_while_the_first_is_preparing_makes_no_second_thread(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let folder = data.path().join("plain");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("No git commit to start a worktree from")
    });
    set_program(cx, window, &kagan, shell());
    cx.simulate_keystrokes(window, "enter enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let listed = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::List {
                project_id: Some(project.id.clone()),
                archived: None,
                limit: None,
                before: None,
            }),
            cx,
        )
    });
    let OpResult::Threads(threads) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("threads");
    };
    assert_eq!(threads.len(), 1);
}

pub(in crate::tests) fn shown(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Option<ThreadId> {
    cx.update(|cx| kagan.read(cx).shown_thread(cx))
}

pub(in crate::tests) fn set_program(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &Entity<Kagan>,
    program: &str,
) {
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| picker.use_custom(program, "", window, cx));
    })
    .unwrap();
}

pub(in crate::tests) fn environment_of(
    cx: &mut TestAppContext,
    thread: &ThreadId,
) -> kg_domain::Environment {
    let id = thread_record(cx, thread)
        .environment_id
        .expect("an environment");
    let task = cx.update(|cx| Workspace::dispatch(Op::Environment(EnvironmentOp::Get { id }), cx));
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Environment(environment) => environment,
        other => panic!("{other:?}"),
    }
}

#[gpui_kit::test]
fn a_failed_branch_read_shows_its_error_and_starts_nothing(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    std::fs::remove_dir_all(&repo).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("read the git branches")
    });
    assert!(shows(cx, window, "folder-retry"));
    assert!(!shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    cx.update_window(window, |_, window, cx| {
        _ = crate::ui_dispatch::click(window, &ElementId::Name("start".into()), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert!(threads_in(cx, &project.id).is_empty());
}

#[gpui_kit::test]
fn a_worktree_starts_from_the_branch_on_screen_after_a_switch(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    let pinned = head_commit(&repo);
    git(&repo, &["checkout", "-q", "-b", "elsewhere"]);
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.base_branch.as_deref(), Some(pinned.as_str()));
    assert_eq!(head_commit(&environment.path), pinned);
}

#[gpui_kit::test]
fn closing_the_window_before_a_worktree_is_prepared_still_finishes_it(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    // Press Start without letting the executor run: the start is queued, not yet prepared. Then
    // close the window and drop the view, so the start has no window left to report to.
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::click(window, &ElementId::Name("start".into()), cx).unwrap();
    })
    .unwrap();
    let view = kagan.downgrade();
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    drop(kagan);
    cx.run_until_parked();
    assert!(
        view.upgrade().is_none(),
        "the closed window's view is released"
    );
    wait_until(cx, |cx| threads_in(cx, &project.id).len() == 1);
    let thread = threads_in(cx, &project.id).remove(0);
    let environment = environment_of(cx, &thread.id);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.status, EnvironmentStatus::Ready);
    assert_eq!(
        std::fs::read_dir(data.path().join("worktrees"))
            .unwrap()
            .count(),
        1
    );
}

#[gpui_kit::test]
fn a_detached_head_offers_a_worktree_from_its_commit(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let branch = current_branch(&repo);
    git(&repo, &["checkout", "-q", "--detach"]);
    // No local branch is left: the worktree must start from the commit HEAD points at.
    git(&repo, &["branch", "-D", &branch]);
    let head = head_commit(&repo);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.base_branch.as_deref(), Some(head.as_str()));
}

#[gpui_kit::test]
fn retry_recovers_a_failed_branch_read_through_its_action(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    std::fs::remove_dir_all(&repo).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("read the git branches")
    });
    init_repo(data.path());
    cx.simulate_keystrokes(window, &chord("RetryBranches"));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(
        environment_of(cx, &started).provider,
        EnvironmentProvider::Worktree
    );
}

pub(in crate::tests) fn head_commit(repo: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

#[gpui_kit::test]
fn the_folder_actions_pick_the_folder_from_the_keyboard(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    cx.simulate_keystrokes(window, &chord("UseProjectFolder"));
    cx.simulate_keystrokes(window, &chord("UseWorktreeFolder"));
    cx.simulate_keystrokes(window, &chord("UseProjectFolder"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(
        environment_of(cx, &started).provider,
        EnvironmentProvider::Project
    );
}

#[gpui_kit::test]
fn a_worktree_can_start_from_another_local_branch(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let (base, base_commit, topic) = other_branch(&repo);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "branch-from"));
    assert_eq!(
        label(cx, window, "branch-from"),
        format!("Branch from: {base}")
    );
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-pick-topic"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "branch-pick-topic"));
    assert!(shows(cx, window, "folder-worktree"));
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-pick-topic"));
    click(cx, window, "branch-pick-topic");
    wait_until(cx, |cx| {
        label(cx, window, "branch-from") == "Branch from: topic"
    });
    set_program(cx, window, &kagan, shell());
    wait_until(cx, |cx| {
        super::setup::disabled(cx, window, "start") == Some(false)
    });
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
    assert_eq!(environment.base_branch.as_deref(), Some(topic.as_str()));
    assert_eq!(head_commit(&environment.path), topic);
    assert_eq!(current_branch(&repo), base);
    assert_eq!(head_commit(&repo), base_commit);
}

#[gpui_kit::test]
fn the_project_folder_can_check_out_another_local_branch(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let (_base, _base_commit, topic) = other_branch(&repo);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    click(cx, window, "folder-project");
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-checkout"));
    click(cx, window, "branch-checkout");
    wait_until(cx, |cx| shows(cx, window, "branch-pick-topic"));
    click(cx, window, "branch-pick-topic");
    wait_until(cx, |cx| {
        label(cx, window, "branch-from") == "Checkout: topic"
    });
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&repo).unwrap());
    assert_eq!(current_branch(&repo), "topic");
    assert_eq!(head_commit(&repo), topic);
}

#[gpui_kit::test]
fn a_dirty_project_folder_stays_on_its_branch(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let base = current_branch(&repo);
    let base_commit = head_commit(&repo);
    std::fs::write(repo.join("dirty.txt"), "changed\n").unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    click(cx, window, "folder-project");
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-block"));
    assert_eq!(
        label(cx, window, "branch-block"),
        "Commit or stash the uncommitted changes in this checkout to create or switch branches."
    );
    assert_eq!(super::setup::disabled(cx, window, "branch-new"), Some(true));
    assert_eq!(
        super::setup::disabled(cx, window, "branch-checkout"),
        Some(true)
    );
    assert_eq!(
        super::setup::disabled(cx, window, "branch-current"),
        Some(false)
    );
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    assert_eq!(current_branch(&repo), base);
    assert_eq!(head_commit(&repo), base_commit);
}

fn other_branch(repo: &Path) -> (String, String, String) {
    let base = current_branch(repo);
    let base_commit = head_commit(repo);
    git(repo, &["checkout", "-q", "-b", "topic"]);
    git(repo, &["commit", "--allow-empty", "-qm", "topic"]);
    let topic = head_commit(repo);
    git(repo, &["checkout", "-q", &base]);
    (base, base_commit, topic)
}

#[gpui_kit::test]
fn escape_closes_the_branch_menu_before_the_start_screen(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let _ = other_branch(&repo);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "branch-from"));
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-pick-topic"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "branch-pick-topic"));
    assert!(shows(cx, window, "folder-worktree"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "folder-worktree"));
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "branch-from"));
    click(cx, window, "folder-project");
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-current"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "branch-current"));
    assert!(shows(cx, window, "folder-project"));
    click(cx, window, "folder-worktree");
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-pick-topic"));
    click(cx, window, "program-search");
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "branch-pick-topic"));
    assert!(shows(cx, window, "folder-worktree"));
}

pub(in crate::tests) fn current_branch(repo: &Path) -> String {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

pub(in crate::tests) fn threads_in(
    cx: &mut TestAppContext,
    project: &ProjectId,
) -> Vec<kg_domain::Thread> {
    let listed = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::List {
                project_id: Some(project.clone()),
                archived: None,
                limit: None,
                before: None,
            }),
            cx,
        )
    });
    let OpResult::Threads(threads) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("threads");
    };
    threads
}
