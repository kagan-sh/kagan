use super::chord;
use crate::kagan::screen_read;

use gpui_kit::InputEvent as _;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, TestAppContext, WindowBounds, px, size,
};
use kg_core::Workspace;
use kg_domain::{EnvironmentProvider, ProjectId, ThreadId, ThreadStatus};

use crate::kagan::{NewThread, StopThread};

use super::{
    add_project, click, commit_setup, committed_repo, environment_of, label, open, run_shell,
    selected_of, set_program, setup, shell, shown, shows, thread_status, wait_until,
};

#[gpui_kit::test]
fn stop_ends_the_program(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(&id))
            .is_some()
    );

    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| {
        cx.update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .terminal_process_id(&id)
                .is_none()
        })
    });
    assert_eq!(thread_status(cx, &id), ThreadStatus::Idle);
    wait_until(cx, |cx| shows(cx, window, "ended"));
    assert!(!shows(cx, window, "stop"));
}

#[gpui_kit::test]
fn the_public_entry_point_opens_one_window(cx: &mut TestAppContext) {
    let _data = setup(cx);
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn the_window_is_titled_kagan_and_opens_at_its_size(cx: &mut TestAppContext) {
    let options = cx.update(crate::kagan::window_options);
    let title = options.titlebar.and_then(|bar| bar.title);
    assert_eq!(title.as_deref(), Some("Kagan"));
    let Some(WindowBounds::Windowed(bounds)) = options.window_bounds else {
        panic!("the window opens windowed");
    };
    assert_eq!(bounds.size, size(px(1120.), px(720.)));
}

#[gpui_kit::test]
fn the_new_thread_shortcut_asks_for_a_program(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, &chord("NewThread"));
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
}

#[gpui_kit::test]
fn new_thread_asks_for_a_program_and_starts_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    cx.dispatch_action(window, NewThread);
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("prj_kagan".into()))
    );
    assert_eq!(
        label(cx, window, "start-prompt"),
        "New thread in Kagan project"
    );
    assert_eq!(selected_of(cx, window, "project-prj_kagan"), Some(true));
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| picker.use_custom(shell(), "", window, cx));
    })
    .unwrap();
    click(cx, window, "start");
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)).is_some()
    });
    let started = cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap();
    assert!(
        cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(&started))
            .is_some()
    );
}

#[gpui_kit::test]
fn a_folder_that_is_not_git_starts_a_thread_in_itself(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let folder = data.path().join("plain");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("No git commit to start a worktree from")
    });
    assert!(!shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&folder).unwrap());
}

#[gpui_kit::test]
fn a_git_folder_starts_a_worktree_by_default(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-worktree"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Worktree);
}

#[gpui_kit::test]
fn the_project_folder_choice_runs_in_the_git_folder(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    click(cx, window, "folder-project");
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&repo).unwrap());
}

#[gpui_kit::test]
fn the_runs_in_sentence_says_where_each_choice_writes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "worktree-hint"));
    let hint = label(cx, window, "worktree-hint");
    assert!(hint.starts_with("New folder on a branch from "), "{hint}");
    assert!(hint.ends_with("/worktrees."), "{hint}");
    click(cx, window, "folder-project");
    wait_until(cx, |cx| shows(cx, window, "project-hint"));
    assert_eq!(
        label(cx, window, "project-hint"),
        "Changes land in the project folder."
    );
}

#[gpui_kit::test]
fn cancel_leaves_the_new_thread_screen(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "start-cancel"));
    click(cx, window, "start-cancel");
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_none()
    });
    assert!(!shows(cx, window, "start"));
}

fn end_shell_thread(cx: &mut TestAppContext, window: AnyWindowHandle) -> ThreadId {
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| shows(cx, window, "ended"));
    id
}

#[gpui_kit::test]
fn new_thread_here_is_offered_only_by_an_ended_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(!shows(cx, window, "new-thread-here"));
    cx.dispatch_action(window, StopThread);
    wait_until(cx, |cx| shows(cx, window, "new-thread-here"));
}

#[gpui_kit::test]
fn new_thread_here_opens_the_start_screen_with_the_folder_fixed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    end_shell_thread(cx, window);
    click(cx, window, "new-thread-here");
    wait_until(cx, |cx| shows(cx, window, "reuse-hint"));
    let hint = label(cx, window, "reuse-hint");
    assert!(hint.starts_with("Runs in "), "{hint}");
    assert!(hint.contains("the folder of “"), "{hint}");
    assert!(!shows(cx, window, "folder-worktree"));
    assert!(!shows(cx, window, "folder-project"));
    assert!(!shows(cx, window, "branch-from"));
}

#[gpui_kit::test]
fn start_in_new_thread_here_runs_in_the_ended_threads_folder(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let ended = end_shell_thread(cx, window);
    let folder = environment_of(cx, &ended).id;
    click(cx, window, "new-thread-here");
    wait_until(cx, |cx| shows(cx, window, "reuse-hint"));
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| {
        shown(cx, &kagan).is_some_and(|started| started != ended)
    });
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(environment_of(cx, &started).id, folder);
}

#[gpui_kit::test]
fn escape_in_new_thread_here_goes_back_to_the_ended_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let ended = end_shell_thread(cx, window);
    click(cx, window, "new-thread-here");
    wait_until(cx, |cx| shows(cx, window, "reuse-hint"));
    press_key(cx, window, "escape");
    wait_until(cx, |cx| shown(cx, &kagan) == Some(ended.clone()));
}

#[gpui_kit::test]
fn escape_leaves_the_new_thread_screen_from_a_choice(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    for _ in 0..40 {
        if choice_focused(cx, window, "folder-project-button") == Some(true) {
            break;
        }
        cx.update_window(window, |_, window, cx| window.focus_next(cx))
            .unwrap();
        cx.run_until_parked();
    }
    assert_eq!(
        choice_focused(cx, window, "folder-project-button"),
        Some(true)
    );
    press_key(cx, window, "escape");
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_none()
    });
}

/// The bounds of an element, as x, y, width and height in points.
fn bounds(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &str,
) -> Option<(f32, f32, f32, f32)> {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(id.into())).map(|element| {
            let bounds = element.bounds;
            (
                f32::from(bounds.origin.x),
                f32::from(bounds.origin.y),
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
            )
        })
    })
    .unwrap()
}

#[gpui_kit::test]
fn show_script_starts_on_the_spine_of_the_note_above_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    commit_setup(&repo, "echo from-commit\n");
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "setup-disclose"));
    let (note_x, _, _, _) = bounds(cx, window, "setup-script").unwrap();
    let (disclose_x, _, _, _) = bounds(cx, window, "setup-disclose").unwrap();
    assert!(
        (disclose_x - note_x).abs() < 0.5,
        "{disclose_x} vs {note_x}"
    );
}

/// Whether the element named `name` is checked, and whether it has focus.
pub(in crate::tests) fn choice_checked(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        gpui_kit::base::test_support::find(window, &[], &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.checked())
    })
    .unwrap()
}

pub(in crate::tests) fn choice_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> Option<bool> {
    cx.update_window(window, |_, window, _| {
        gpui_kit::base::test_support::find(window, &[], &ElementId::Name(name.to_owned().into()))
            .and_then(|element| element.focused())
    })
    .unwrap()
}

/// Presses and releases a key the way the platform does: key down, then key up. A button
/// clicks from the key up, so a key down alone does nothing.
pub(in crate::tests) fn press_key(cx: &mut TestAppContext, window: AnyWindowHandle, key: &str) {
    cx.update_window(window, |_, window, cx| {
        let keystroke = kg_core::parse_key(key).unwrap();
        window.dispatch_event(
            gpui_kit::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(gpui_kit::KeyUpEvent { keystroke }.to_platform_input(), cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_folder_choices_are_named_and_show_which_one_a_start_uses(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    assert_eq!(label(cx, window, "folder-project"), "Project folder");
    assert_eq!(label(cx, window, "folder-worktree"), "New worktree");
    assert!(label(cx, window, "branch-from").starts_with("Branch from: "));
    assert_eq!(selected_of(cx, window, "folder-worktree"), Some(true));
    assert_eq!(selected_of(cx, window, "folder-project"), Some(false));
    assert!(shows(cx, window, "worktree-hint"));
    assert_eq!(
        choice_checked(cx, window, "folder-worktree-button"),
        Some(true)
    );
    click(cx, window, "folder-project");
    wait_until(cx, |cx| {
        choice_checked(cx, window, "folder-project-button") == Some(true)
    });
    assert_eq!(
        choice_checked(cx, window, "folder-worktree-button"),
        Some(false)
    );
    assert_eq!(selected_of(cx, window, "folder-worktree"), Some(false));
    assert_eq!(selected_of(cx, window, "folder-project"), Some(true));
}

#[gpui_kit::test]
fn the_folder_choice_takes_the_keyboard(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    for _ in 0..40 {
        if choice_focused(cx, window, "folder-project-button") == Some(true) {
            break;
        }
        cx.update_window(window, |_, window, cx| window.focus_next(cx))
            .unwrap();
        cx.run_until_parked();
    }
    assert_eq!(
        choice_focused(cx, window, "folder-project-button"),
        Some(true),
        "tab reaches the Project folder choice"
    );
    press_key(cx, window, "enter");
    assert_eq!(
        choice_checked(cx, window, "folder-project-button"),
        Some(true)
    );
    set_program(cx, window, &kagan, shell());
    click(cx, window, "start");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    let environment = environment_of(cx, &started);
    assert_eq!(environment.provider, EnvironmentProvider::Project);
    assert_eq!(environment.path, dunce::canonicalize(&repo).unwrap());
}
