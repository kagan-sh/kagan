use super::chord;
use crate::kagan::screen_read;

use gpui_kit::InputEvent as _;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext, WindowBounds, px, size,
};
use kg_core::Workspace;
use kg_domain::{EnvironmentProvider, ProjectId, ThreadId, ThreadStatus};

use crate::kagan::{Kagan, NewThread, StopThread};

use super::setup::disabled;
use super::{
    add_project, click, commit_setup, committed_repo, environment_of, label, open, run_shell,
    scroll_form_to_end, selected_of, set_program, setup, shell, shown, shows, thread_status,
    wait_until,
};
#[cfg(unix)]
use crate::tests::app::picker::{fixture_programs, start_in_plain_folder};

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
    assert!(options.window_min_size.is_some());
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
    let tail = format!("{}worktrees.", std::path::MAIN_SEPARATOR);
    assert!(hint.ends_with(&tail), "{hint}");
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

// The form's controls in the order Tab reaches them, after the program search.
const FORM_STOPS: [&str; 7] = [
    "custom-command-toggle-button",
    "saved-program-save-button",
    "folder-project-button",
    "folder-worktree-button",
    "branch-from-button",
    "start-cancel-button",
    "start-button-control",
];

#[gpui_kit::test]
fn tab_visits_each_control_of_the_form_in_order_then_escape_leaves(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_git_form(cx);
    click(cx, window, "program-search");
    // The program grid is one stop: its cards take the arrow keys. Save sits before the folder.
    let mut order = Vec::new();
    // The program grid is a stop too, so one more press than the controls reaches the last one.
    for _ in 0..FORM_STOPS.len() + 4 {
        press_key(cx, window, "tab");
        order.extend(
            FORM_STOPS
                .into_iter()
                .find(|name| choice_focused(cx, window, name) == Some(true)),
        );
    }
    assert_eq!(order, FORM_STOPS.to_vec());
    press_key(cx, window, "escape");
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_none()
    });
}

fn thread_count(cx: &mut TestAppContext) -> usize {
    cx.update(|cx| Workspace::global(cx).read(cx).thread_records(cx).len())
}

fn tab_to(cx: &mut TestAppContext, window: AnyWindowHandle, name: &str) {
    for _ in 0..12 {
        if choice_focused(cx, window, name) == Some(true) {
            return;
        }
        press_key(cx, window, "tab");
    }
    panic!("Tab never reached {name}");
}

// A git folder with Shell chosen, so Start is ready.
fn open_git_form(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "start-cancel"));
    set_program(cx, window, &kagan, shell());
    wait_until(cx, |cx| disabled(cx, window, "start") == Some(false));
    (kagan, window, data)
}

// A plain folder with a fake Claude installed. Its program runs in a plain folder, and the first
// installed program is the one Start runs, so Start is ready.
#[cfg(unix)]
fn open_plain_form(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, tempfile::TempDir) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["claude"]);
    start_in_plain_folder(cx, window, data.path());
    wait_until(cx, |cx| disabled(cx, window, "start") == Some(false));
    (kagan, window, data)
}

#[gpui_kit::test]
fn enter_on_a_focused_cancel_leaves_and_starts_no_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_git_form(cx);
    click(cx, window, "program-search");
    tab_to(cx, window, "start-cancel-button");
    let before = thread_count(cx);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_none()
    });
    cx.run_until_parked();
    assert_eq!(thread_count(cx), before);
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_in_the_program_search_starts_exactly_one_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_plain_form(cx);
    click(cx, window, "program-search");
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    cx.run_until_parked();
    assert_eq!(thread_count(cx), 1);
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_on_the_form_itself_starts_one_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_plain_form(cx);
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.focus_start_form(window, cx));
    })
    .unwrap();
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    cx.run_until_parked();
    assert_eq!(thread_count(cx), 1);
}

#[gpui_kit::test]
fn enter_in_the_branch_search_picks_a_branch_and_starts_nothing(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_git_form(cx);
    // The custom command's fields make the form taller than a short window, so the branch control
    // is below the fold until the person scrolls to it.
    scroll_form_to_end(cx, window);
    click(cx, window, "branch-from");
    wait_until(cx, |cx| shows(cx, window, "branch-search"));
    press_key(cx, window, "enter");
    cx.run_until_parked();
    assert_eq!(thread_count(cx), 0);
    assert!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_some()
    );
}

#[gpui_kit::test]
fn the_start_key_badge_shows_the_key_that_starts_the_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_git_form(cx);
    assert_eq!(
        label(cx, window, "start-key"),
        Kbd::format(&kg_core::parse_key("enter").unwrap())
    );
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
    assert!(hint.ends_with(". Changes land there."), "{hint}");
    let heading = label(cx, window, "start-prompt");
    assert!(heading.starts_with("New thread beside “"), "{heading}");
    assert!(heading.ends_with('”'), "{heading}");
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

#[gpui_kit::test]
fn cancel_leaves_the_screen_with_the_branch_menu_open(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "folder-project"));
    click(cx, window, "folder-project");
    click(cx, window, "branch-from");
    scroll_form_to_end(cx, window);
    wait_until(cx, |cx| shows(cx, window, "branch-checkout"));
    click(cx, window, "start-cancel");
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_none()
    });
}

#[gpui_kit::test]
fn the_program_list_has_a_caption_like_runs_in(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "program-search"));
    assert_eq!(label(cx, window, "program-caption"), "Program");
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
    scroll_form_to_end(cx, window);
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
