use std::fmt::Write as _;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext};
use kg_core::Workspace;
use kg_domain::EnvironmentStatus;

use super::{
    click, committed_repo, environment_of, git, head_commit, label, only_worktree, open,
    set_program, shell, shown, shows, wait_until, worktree_dirs,
};

pub(in crate::tests) fn disabled(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.disabled)
    })
    .unwrap()
}

pub(in crate::tests) fn commit_setup(repo: &Path, body: &str) {
    let path = repo.join(".kagan/setup");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, body).unwrap();
    git(repo, &["add", "--", ".kagan/setup"]);
    git(
        repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "setup",
        ],
    );
}

#[gpui_kit::test]
fn the_start_screen_names_setup_before_start(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    set_program(cx, window, &kagan, "");
    wait_until(cx, |cx| {
        label(cx, window, "setup-script").contains(".kagan/setup")
    });
    assert_eq!(disabled(cx, window, "start"), Some(true));
    assert!(shown(cx, &kagan).is_none());
    set_program(cx, window, &kagan, shell());
    wait_until(cx, |cx| disabled(cx, window, "start") == Some(false));
    assert!(label(cx, window, "setup-script").contains(".kagan/setup"));
}

#[gpui_kit::test]
fn the_project_folder_choice_does_not_name_setup(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    click(cx, window, "folder-project");
    cx.run_until_parked();
    assert!(!shows(cx, window, "setup-script"));
    click(cx, window, "folder-worktree");
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    let _ = kagan;
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_setup_link_is_named_and_not_offered_as_a_run(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let target = data.path().join("real-setup.sh");
    std::fs::write(&target, "exit 1\n").unwrap();
    std::fs::create_dir_all(repo.join(".kagan")).unwrap();
    std::os::unix::fs::symlink(&target, repo.join(".kagan/setup")).unwrap();
    git(&repo, &["add", "--", ".kagan/setup"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Kagan",
            "-c",
            "user.email=kagan@example.com",
            "commit",
            "-qm",
            "link",
        ],
    );
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        let text = label(cx, window, "setup-script");
        text.contains(".kagan/setup") && text.contains("will not")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn retry_runs_a_failed_setup_again_without_a_second_worktree(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(
        &repo,
        "if [ -f .kagan/once ]; then exit 0; fi\ntouch .kagan/once\nexit 1\n",
    );
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| {
        label(cx, window, "setup-failure").contains(".kagan/setup")
    });
    assert_eq!(
        label(cx, window, "setup-retry"),
        "Retry setup in this worktree"
    );
    assert!(shown(cx, &kagan).is_none());
    assert_eq!(
        std::fs::read_dir(data.path().join("worktrees"))
            .unwrap()
            .count(),
        1
    );
    click(cx, window, "setup-retry");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    assert_eq!(
        std::fs::read_dir(data.path().join("worktrees"))
            .unwrap()
            .count(),
        1
    );
    let started = shown(cx, &kagan).unwrap();
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(&started))
            .is_some()
    );
}

#[gpui_kit::test]
fn a_deleted_setup_is_still_named_from_the_commit(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    std::fs::remove_file(repo.join(".kagan/setup")).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    set_program(cx, window, &kagan, "");
    wait_until(cx, |cx| {
        label(cx, window, "setup-script").contains(".kagan/setup")
    });
    assert_eq!(disabled(cx, window, "start"), Some(true));
}

#[gpui_kit::test]
fn an_uncommitted_setup_is_not_named(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let path = repo.join(".kagan/setup");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "exit 1\n").unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    cx.run_until_parked();
    assert!(!shows(cx, window, "setup-script"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_uncommitted_link_does_not_replace_the_named_script(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 0\n");
    std::fs::remove_file(repo.join(".kagan/setup")).unwrap();
    let target = data.path().join("other-setup.sh");
    std::fs::write(&target, "exit 1\n").unwrap();
    std::os::unix::fs::symlink(&target, repo.join(".kagan/setup")).unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        let text = label(cx, window, "setup-script");
        text.contains("Runs .kagan/setup") && !text.contains("will not")
    });
}

#[gpui_kit::test]
fn a_moved_branch_runs_the_setup_the_screen_named(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "touch named-ran\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "setup-script").contains("Runs .kagan/setup")
    });
    let pinned = head_commit(&repo);
    commit_setup(&repo, "touch moved-ran\n");
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.status, EnvironmentStatus::Ready);
    assert_eq!(head_commit(&environment.path), pinned);
    assert!(environment.path.join("named-ran").is_file());
    assert!(!environment.path.join("moved-ran").exists());
}

#[gpui_kit::test]
fn the_start_screen_can_show_the_pinned_setup_script(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "echo from-commit\n");
    std::fs::write(repo.join(".kagan/setup"), "echo from-worktree\n").unwrap();
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-disclose"));
    assert_eq!(label(cx, window, "setup-disclose"), "Show script");
    assert!(!shows(cx, window, "setup-body"));
    click(cx, window, "setup-disclose");
    wait_until(cx, |cx| shows(cx, window, "setup-body"));
    let script = label(cx, window, "setup-body");
    assert!(script.contains("echo from-commit"), "{script}");
    assert!(!script.contains("from-worktree"), "{script}");
    assert_eq!(label(cx, window, "setup-disclose"), "Hide script");
}

#[gpui_kit::test]
fn a_long_setup_script_is_capped(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    let mut body = String::new();
    for n in 0..45 {
        writeln!(body, "echo line-{n}").unwrap();
    }
    commit_setup(&repo, &body);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-disclose"));
    click(cx, window, "setup-disclose");
    wait_until(cx, |cx| shows(cx, window, "setup-body"));
    let script = label(cx, window, "setup-body");
    assert!(script.contains("echo line-0"), "{script}");
    assert!(script.contains("echo line-39"), "{script}");
    assert!(!script.contains("echo line-40"), "{script}");
    assert_eq!(
        label(cx, window, "setup-truncated"),
        "The rest of the script is not shown."
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_changed_script_hides_retry_and_leaves_start(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "exit 1\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-script"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shows(cx, window, "setup-retry"));
    let old = only_worktree(data.path());
    std::fs::write(old.join(".kagan/setup"), "echo edited\n").unwrap();
    click(cx, window, "setup-retry");
    wait_until(cx, |cx| {
        label(cx, window, "setup-failure").contains("Start will use the version shown")
    });
    let failure = label(cx, window, "setup-failure");
    assert!(failure.contains(".kagan/setup changed"), "{failure}");
    assert!(!shows(cx, window, "setup-retry"));
    assert!(shows(cx, window, "start"));
    assert!(!old.exists());
    assert!(worktree_dirs(data.path()).is_empty());
    click(cx, window, "start");
    wait_until(cx, |cx| shows(cx, window, "setup-retry"));
    let fresh = only_worktree(data.path());
    assert_eq!(fresh, old);
}
