use crate::kagan::screen_read;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px, size};
use kg_domain::ProjectId;

#[cfg(unix)]
use crate::kagan::Kagan;
use crate::tests::kagan::{add_project, click, label, open, shows, wait_until};
#[cfg(unix)]
use crate::tests::kagan::{
    committed_repo, focused_of, role_of, selected_of, shown, thread_record, threads_in, value_of,
};
#[cfg(unix)]
use gpui_kit::Entity;
#[cfg(unix)]
use gpui_kit::Role;
#[cfg(unix)]
use kg_core::Workspace;
#[cfg(unix)]
use kg_domain::{EnvironmentSpec, Op, OpResult, ProgramOp, Run, ThreadOp};
#[cfg(unix)]
use std::path::PathBuf;

/// Points this workspace's program discovery at fake programs named `names`, and at a fake shell,
/// in a new folder under `data`. Each fake prints its own name and its arguments, so a thread's screen shows which
/// program ran. Returns the folder.
#[cfg(unix)]
pub(crate) fn fixture_programs(cx: &mut TestAppContext, data: &Path, names: &[&str]) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = data.join("programs");
    std::fs::create_dir_all(&bin).unwrap();
    for name in names.iter().copied().chain(["fixture-shell"]) {
        let path = bin.join(name);
        let script = format!(
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then echo \"{name} 1.2.3 (fixture build)\"; else echo \"FIXTURE {name} $*\"; fi\n"
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let shell = bin.join("fixture-shell");
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(&bin, &shell);
        });
    });
    bin
}

/// Opens the start screen of a plain folder project, once the picker has its programs.
pub(crate) fn start_in_plain_folder(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    data: &Path,
) -> ProjectId {
    let folder = data.join("plain");
    std::fs::create_dir_all(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| {
        label(cx, window, "folder").contains("No git commit to start a worktree from")
    });
    wait_until(cx, |cx| shows(cx, window, "program-list"));
    project.id
}

#[cfg(unix)]
fn picker(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Entity<crate::kagan::picker::Picker> {
    cx.update(|cx| kagan.read(cx).picker_entity())
}

#[cfg(unix)]
fn command(cx: &mut TestAppContext, kagan: &Entity<Kagan>) -> Result<String, String> {
    let picker = picker(cx, kagan);
    cx.update(|cx| picker.read(cx).command(cx))
}

#[cfg(unix)]
fn programs(cx: &mut TestAppContext) -> Vec<kg_domain::Program> {
    let listed =
        cx.update(|cx| Workspace::dispatch(Op::Program(ProgramOp::List { project_id: None }), cx));
    let OpResult::Programs(list) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("programs");
    };
    list.programs
}

#[cfg(unix)]
fn default_program(cx: &mut TestAppContext) -> Option<String> {
    let listed =
        cx.update(|cx| Workspace::dispatch(Op::Program(ProgramOp::List { project_id: None }), cx));
    let OpResult::Programs(list) = cx.foreground_executor().block_test(listed).unwrap() else {
        panic!("programs");
    };
    list.default
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

#[cfg(unix)]
#[gpui_kit::test]
fn the_list_names_every_installed_agent_and_the_shell(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["claude", "codex", "gemini"]);
    start_in_plain_folder(cx, window, data.path());
    let listed = programs(cx);
    let installed = |command: &str| {
        listed
            .iter()
            .find(|program| program.command == command)
            .map(kg_domain::Program::installed)
    };
    assert_eq!(installed("claude"), Some(true));
    assert_eq!(installed("codex"), Some(true));
    // A third agent on the PATH is listed. One that is missing and not always listed is not.
    assert_eq!(installed("gemini"), Some(true));
    assert_eq!(installed("opencode"), None);
    let shell = bin.join("fixture-shell").display().to_string();
    assert!(
        listed
            .iter()
            .any(|program| program.command == shell && program.installed())
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_custom_command_typed_in_its_fields_runs_as_one_program(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["gemini"]);
    let _ = start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    let program = bin.join("gemini").display().to_string();
    cx.simulate_input(window, &program);
    cx.simulate_keystrokes(window, "tab");
    cx.simulate_input(window, "hello");
    assert_eq!(command(cx, &kagan), Ok(format!("{program} hello")));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(
        thread_record(cx, &started).provider,
        format!("{program} hello")
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_row_click_starts_that_program_and_closes_the_custom_command(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["claude"]);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_input(window, "/does/not/exist");
    click(cx, window, "program-claude");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(thread_record(cx, &started).provider, "claude");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_search_keeps_only_the_matching_program_selected(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex", "gemini"]);
    start_in_plain_folder(cx, window, data.path());
    let picker = picker(cx, &kagan);
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    cx.simulate_input(window, "gemini");
    cx.run_until_parked();
    assert_eq!(command(cx, &kagan), Ok("gemini".to_owned()));
}

#[cfg(unix)]
#[gpui_kit::test]
fn reopening_keeps_the_search_and_its_selection(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex", "gemini"]);
    let project = start_in_plain_folder(cx, window, data.path());
    let picker = picker(cx, &kagan);
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    cx.simulate_input(window, "gemini");
    cx.run_until_parked();
    // Reopen: the same project's start screen is shown again, and its programs are read again.
    click(cx, window, &format!("project-new-{}", project.0));
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "program-list"));
    cx.run_until_parked();
    assert_eq!(command(cx, &kagan), Ok("gemini".to_owned()));
}

#[cfg(unix)]
#[gpui_kit::test]
fn escape_closes_the_custom_command_from_its_fields_and_from_the_list(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    // From the Program field.
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_keystrokes(window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "custom-program"));
    // From the Arguments field.
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    click(cx, window, "custom-arguments");
    cx.simulate_keystrokes(window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "custom-program"));
    // From the list, where focus goes back when the custom command closes.
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    let picker = picker(cx, &kagan);
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    cx.simulate_keystrokes(window, "escape");
    wait_until(cx, |cx| !shows(cx, window, "custom-program"));
    assert_eq!(command(cx, &kagan), Ok("codex".to_owned()));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_selected_row_is_marked_on_screen_and_in_words(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    assert_eq!(label(cx, window, "program-codex"), "Codex, selected");
    let (x, y, width, height) = bounds(cx, window, "program-codex").unwrap();
    let (color, scale) = cx
        .update_window(window, |_, window, cx| {
            use gpui_kit::component::ActiveTheme as _;
            (cx.theme().secondary, window.scale_factor())
        })
        .unwrap();
    let quads = cx
        .update_window(window, |_, window, _| window.painted_quads())
        .unwrap();
    let center = ((x + width / 2.) * scale, (y + height / 2.) * scale);
    assert!(
        quads.iter().any(|quad| {
            let bounds = quad.bounds;
            quad.background == color.into()
                && bounds.origin.x.as_f32() <= center.0
                && bounds.origin.y.as_f32() <= center.1
                && bounds.origin.x.as_f32() + bounds.size.width.as_f32() >= center.0
                && bounds.origin.y.as_f32() + bounds.size.height.as_f32() >= center.1
        }),
        "selected row has the raised fill"
    );
    let shell = bin.join("fixture-shell").display().to_string();
    assert_eq!(label(cx, window, &format!("program-name-{shell}")), "Shell");
    cx.simulate_keystrokes(window, "down");
    wait_until(cx, |cx| {
        label(cx, window, &format!("program-name-{shell}")) == "Shell, selected"
    });
    assert_eq!(label(cx, window, "program-codex"), "Codex");
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_search_with_no_match_says_what_to_do_and_gives_no_second_reason(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    search(cx, window, &kagan, "zzz");
    wait_until(cx, |cx| label(cx, window, "program-empty").contains("zzz"));
    assert_eq!(
        label(cx, window, "program-empty"),
        "No program matches “zzz”. Clear the search to see them all."
    );
    assert!(!shows(cx, window, "program-reason"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_known_agent_row_shows_its_version_and_a_shell_row_shows_its_command(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    wait_until(cx, |cx| {
        label(cx, window, "program-version-codex") == "1.2.3"
    });
    assert!(!shows(cx, window, "program-command-codex"));
    let shell = bin.join("fixture-shell").display().to_string();
    assert_eq!(
        label(cx, window, &format!("program-command-{shell}")),
        shell
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_start_title_stays_put_when_the_custom_command_opens(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    let (_, closed_y, _, _) = bounds(cx, window, "start-prompt").unwrap();
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    let (_, open_y, _, _) = bounds(cx, window, "start-prompt").unwrap();
    assert_close(open_y, closed_y, "the title");
}

#[cfg(unix)]
#[gpui_kit::test]
fn start_sits_under_the_last_field_and_on_screen_in_a_worktree(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let (_repo, project) = committed_repo(cx, data.path());
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "worktree-hint"));
    let (_, hint_y, _, hint_height) = bounds(cx, window, "worktree-hint").unwrap();
    let (_, start_y, _, start_height) = bounds(cx, window, "start").unwrap();
    let gap = start_y - (hint_y + hint_height);
    assert!((0. ..=24.).contains(&gap), "gap {gap}");
    assert!(
        start_y + start_height <= 720.,
        "start ends at {}",
        start_y + start_height
    );
    assert!(shows(cx, window, "start"));
}

#[gpui_kit::test]
fn the_program_form_keeps_a_readable_width_and_reads_top_down(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    for width in [1120., 528.] {
        cx.simulate_window_resize(window, size(px(width), px(720.)));
        cx.run_until_parked();
        let (list_x, list_y, list_width, _) = bounds(cx, window, "program-list").unwrap();
        // The form is capped at 40rem, so it never stretches across a wide window.
        assert!(list_width <= 40. * 16. + 1., "list {list_width} at {width}");
        assert!(
            list_x + list_width <= width + 0.5,
            "list leaves the window at {width}"
        );
        let (_, start_y, _, _) = bounds(cx, window, "start").unwrap();
        assert!(
            start_y > list_y,
            "Start sits below the program list at {width}"
        );
    }
}

#[gpui_kit::test]
fn an_empty_center_offers_a_personal_thread_and_a_project(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    assert!(shows(cx, window, "new-personal-thread"));
    assert!(shows(cx, window, "welcome-add-project"));
    assert_eq!(label(cx, window, "welcome-title"), "");
    click(cx, window, "new-personal-thread");
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        Some(ProjectId("personal".into()))
    );
}

#[gpui_kit::test]
fn escape_leaves_the_program_picker(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    click(cx, window, "new-personal-thread");
    wait_until(cx, |cx| shows(cx, window, "program-search"));
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "custom-program"));
    assert!(shows(cx, window, "program-search"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );
    assert!(shows(cx, window, "new-personal-thread"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn escape_in_the_program_list_leaves_the_screen_and_keeps_the_program(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    let picker = picker(cx, &kagan);
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus_list(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(command(cx, &kagan), Ok("codex".to_owned()));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "program-list"));
    assert_eq!(
        cx.update(|cx| screen_read::start_project(kagan.read(cx))),
        None
    );
    assert_eq!(command(cx, &kagan), Ok("codex".to_owned()));
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_archived_program_stays_the_recent_default(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let data = data.path().to_path_buf();
    let bin = fixture_programs(cx, &data, &["codex", "gemini"]);
    let run = Run {
        project_id: ProjectId("personal".into()),
        program: bin.join("gemini").display().to_string(),
        title: None,
        environment: EnvironmentSpec::Personal,
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
    let archive = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Archive {
                id: thread.id.clone(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(archive).unwrap();
    cx.run_until_parked();
    let gemini = bin.join("gemini").display().to_string();
    // Without the archived thread the default would be the first installed agent, codex.
    assert_eq!(default_program(cx), Some(gemini.clone()));
    // A restart reopens the same data folder, and the recent program is still the default.
    let reopened = cx.update(|cx| {
        Workspace::open(
            data.as_path().into(),
            std::env::current_exe().unwrap(),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(reopened).unwrap();
    cx.run_until_parked();
    assert_eq!(default_program(cx), Some(gemini));
}

#[gpui_kit::test]
fn the_custom_command_key_opens_and_closes_the_fields(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    assert!(!shows(cx, window, "custom-program"));
    cx.simulate_keystrokes(window, &crate::tests::kagan::chord("ToggleCustomCommand"));
    cx.run_until_parked();
    assert!(shows(cx, window, "custom-program"));
    cx.simulate_keystrokes(window, &crate::tests::kagan::chord("ToggleCustomCommand"));
    cx.run_until_parked();
    assert!(!shows(cx, window, "custom-program"));
}

#[gpui_kit::test]
fn an_empty_custom_command_says_why_start_is_off(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| picker.use_custom("", "", window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    // Shown once, under the field it is about and above Arguments.
    assert!(!shows(cx, window, "program-reason"));
    assert!(!shows(cx, window, "custom-preview"));
    assert_eq!(
        label(cx, window, "program-feedback"),
        "Enter a program to start."
    );
    let (_, program_y, _, program_height) = bounds(cx, window, "custom-program").unwrap();
    let (_, arguments_y, _, _) = bounds(cx, window, "custom-arguments").unwrap();
    let (_, hint_y, _, hint_height) = bounds(cx, window, "program-feedback").unwrap();
    assert!(hint_y > program_y, "the hint sits under the Program field");
    assert!(
        hint_y + hint_height <= program_y + program_height + 0.5,
        "the hint stays inside the Program block"
    );
    assert!(hint_y < arguments_y, "the hint is above Arguments");
}

#[gpui_kit::test]
fn the_custom_command_button_is_named_in_both_states(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    assert_eq!(
        label(cx, window, "custom-command-toggle"),
        "Custom command…"
    );
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    assert_eq!(
        label(cx, window, "custom-command-toggle"),
        "Hide custom command"
    );
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| !shows(cx, window, "custom-program"));
    assert_eq!(
        label(cx, window, "custom-command-toggle"),
        "Custom command…"
    );
}

#[gpui_kit::test]
fn the_custom_preview_says_the_argument_count_in_plain_words(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    for (arguments, preview) in [
        ("", "Runs claude"),
        ("--fast", "Runs claude with 1 argument"),
        ("--fast --x", "Runs claude with 2 arguments"),
    ] {
        cx.update_window(window, |_, window, cx| {
            let picker = kagan.read(cx).picker_entity();
            picker.update(cx, |picker, cx| {
                picker.use_custom("claude", arguments, window, cx);
            });
        })
        .unwrap();
        cx.run_until_parked();
        assert_eq!(label(cx, window, "custom-preview"), preview);
    }
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_command_line_in_program_shows_a_hint(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["claude"]);
    start_in_plain_folder(cx, window, data.path());
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| {
            picker.use_custom("bash -c 'echo hi'", "", window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(
        label(cx, window, "program-feedback"),
        "Put arguments in Arguments. Quote a program path containing spaces."
    );
    assert!(!shows(cx, window, "custom-preview"));
    assert_eq!(selected_of(cx, window, "program-claude"), Some(false));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_start_that_works_clears_an_earlier_start_error(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["claude"]);
    start_in_plain_folder(cx, window, data.path());
    let use_program = |cx: &mut TestAppContext, program: &'static str| {
        cx.update_window(window, |_, window, cx| {
            let picker = kagan.read(cx).picker_entity();
            picker.update(cx, |picker, cx| picker.use_custom(program, "", window, cx));
        })
        .unwrap();
        cx.run_until_parked();
    };
    use_program(cx, "/nonexistent/kagan-missing-program");
    click(cx, window, "start");
    wait_until(cx, |cx| shows(cx, window, "note"));
    assert!(label(cx, window, "note").contains("Could not start the thread"));
    use_program(cx, "claude");
    click(cx, window, "start");
    wait_until(cx, |cx| !shows(cx, window, "note"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_broken_argument_is_said_under_arguments(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    start_in_plain_folder(cx, window, data.path());
    cx.update_window(window, |_, window, cx| {
        let picker = kagan.read(cx).picker_entity();
        picker.update(cx, |picker, cx| {
            picker.use_custom("claude", "'open", window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "program-feedback"));
    assert_eq!(
        label(cx, window, "arguments-feedback"),
        "The arguments have an unfinished quote."
    );
    let (_, arguments_y, _, arguments_height) = bounds(cx, window, "custom-arguments").unwrap();
    let (_, hint_y, _, hint_height) = bounds(cx, window, "arguments-feedback").unwrap();
    assert!(
        hint_y > arguments_y,
        "the hint sits under the Arguments field"
    );
    assert!(
        hint_y + hint_height <= arguments_y + arguments_height + 0.5,
        "the hint stays inside the Arguments block"
    );
}

/// Seven known agents, plus the shell: a search can leave one row, or overflow the list.
#[cfg(unix)]
const ROW_NAMES: [&str; 7] = [
    "claude",
    "codex",
    "gemini",
    "opencode",
    "cursor-agent",
    "amp",
    "copilot",
];

#[cfg(unix)]
fn list_height(cx: &mut TestAppContext, window: AnyWindowHandle) -> f32 {
    bounds(cx, window, "program-list").unwrap().3
}

/// The first row's height: the distance from its label down to the next row's label.
#[cfg(unix)]
fn row_height(cx: &mut TestAppContext, window: AnyWindowHandle) -> f32 {
    let (_, first, _, _) = bounds(cx, window, "program-name-claude").unwrap();
    let (_, second, _, _) = bounds(cx, window, "program-name-codex").unwrap();
    second - first
}

#[cfg(unix)]
fn search(cx: &mut TestAppContext, window: AnyWindowHandle, kagan: &Entity<Kagan>, text: &str) {
    let picker = picker(cx, kagan);
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    cx.simulate_input(window, text);
    cx.run_until_parked();
}

#[cfg(unix)]
fn assert_close(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() < 0.5,
        "{what}: {actual} is not {expected}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_short_result_is_one_row_and_a_long_one_shows_eight(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &ROW_NAMES);
    start_in_plain_folder(cx, window, data.path());
    let row = row_height(cx, window);
    let all = list_height(cx, window);
    search(cx, window, &kagan, "codex");
    // Eight rows show seven more rows than the one row a search leaves.
    assert_close(
        all - list_height(cx, window),
        7. * row,
        "eight rows minus one",
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_list_longer_than_its_rows_says_how_many_more_there_are(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    fixture_programs(
        cx,
        data.path(),
        &[
            "claude",
            "codex",
            "gemini",
            "opencode",
            "cursor-agent",
            "amp",
            "copilot",
            "kimi",
        ],
    );
    start_in_plain_folder(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "program-more"));
    assert_eq!(
        label(cx, window, "program-more"),
        "1 more. Type to search, or scroll."
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn larger_text_makes_the_rows_taller_and_the_eight_row_cap_follows(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    // kagan has no text-size setting, so larger text is the theme's base font size. It is set
    // before the picker opens, so the rows are measured once, at that size.
    cx.update(|cx| gpui_kit::component::theme::Theme::global_mut(cx).font_size = px(24.));
    fixture_programs(cx, data.path(), &ROW_NAMES);
    start_in_plain_folder(cx, window, data.path());
    let row = row_height(cx, window);
    assert!(row > 28., "rows grow with the text: {row}");
    let all = list_height(cx, window);
    search(cx, window, &kagan, "codex");
    assert_close(
        all - list_height(cx, window),
        7. * row,
        "cap at larger text",
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn escape_closes_the_custom_command_and_enter_starts_exactly_one_thread(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    let project = start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();
    assert!(!shows(cx, window, "custom-program"));
    assert_eq!(label(cx, window, "program-codex"), "Codex, selected");
    assert_eq!(command(cx, &kagan), Ok("codex".to_owned()));
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    assert_eq!(threads_in(cx, &project).len(), 1);
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(thread_record(cx, &started).provider, "codex");
}

#[gpui_kit::test]
fn the_welcome_add_project_opens_the_new_projects_start_screen(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    assert!(shows(cx, window, "welcome-add-project"));
    let folder = data.path().join("picked");
    std::fs::create_dir(&folder).unwrap();
    // The folder prompt hands its folder to this step, so the step is what the test drives.
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |_, cx| {
            crate::kagan::Kagan::add_picked_project(folder.clone(), window, cx);
        });
    })
    .unwrap();
    wait_until(cx, |cx| {
        cx.update(|cx| screen_read::start_project(kagan.read(cx)))
            .is_some()
    });
    let project = cx
        .update(|cx| screen_read::start_project(kagan.read(cx)))
        .unwrap();
    assert_eq!(label(cx, window, "start-prompt"), "New thread in picked");
    assert!(project.0.starts_with("prj_"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_second_workspace_does_not_inherit_the_first_discovery(cx: &mut TestAppContext) {
    let (_kagan, _window, data) = open(cx);
    let first = fixture_programs(cx, data.path(), &["amp"]);
    let first_shell = first.join("fixture-shell").display().to_string();
    assert!(
        programs(cx)
            .iter()
            .any(|program| program.command == first_shell)
    );
    // A second workspace, in its own folder, reads its own PATH and shell.
    let other = tempfile::tempdir().unwrap();
    let opened = cx.update(|cx| {
        Workspace::open(
            other.path().into(),
            std::env::current_exe().unwrap(),
            None,
            cx,
        )
    });
    cx.foreground_executor().block_test(opened).unwrap();
    cx.run_until_parked();
    assert!(
        !programs(cx)
            .iter()
            .any(|program| program.command == first_shell)
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_long_program_list_scrolls_to_its_last_row(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &ROW_NAMES);
    start_in_plain_folder(cx, window, data.path());
    let listed = programs(cx);
    let picker = picker(cx, &kagan);
    cx.update_window(window, |_, window, cx| {
        picker.update(cx, |picker, cx| picker.focus(window, cx));
    })
    .unwrap();
    for _ in 1..listed.len() {
        cx.simulate_keystrokes(window, "down");
        cx.run_until_parked();
    }
    let last = listed.last().expect("programs").command.clone();
    let (_, top, _, height) = bounds(cx, window, "program-list").unwrap();
    let (_, row, _, row_height) = bounds(cx, window, &format!("program-{last}")).unwrap();
    assert!(
        row >= top - 0.5 && row + row_height <= top + height + 0.5,
        "row {row}+{row_height} is outside the list {top}+{height}"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_offscreen_default_program_is_visible_when_the_picker_opens(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &ROW_NAMES);
    let shell = bin.join("fixture-shell").display().to_string();
    let run = Run {
        project_id: ProjectId("personal".into()),
        program: shell.clone(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    cx.foreground_executor().block_test(started).unwrap();
    cx.run_until_parked();
    start_in_plain_folder(cx, window, data.path());
    let row = format!("program-{shell}");
    wait_until(cx, |cx| shows(cx, window, &row));
    assert_eq!(selected_of(cx, window, &row), Some(true));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_version_shows_its_number_without_its_build_suffix_or_full_stop(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["codex", "copilot"]);
    std::fs::write(
        bin.join("codex"),
        "#!/bin/sh\necho \"codex-cli 0.16.0-beta+build.7\"\n",
    )
    .unwrap();
    std::fs::write(
        bin.join("copilot"),
        "#!/bin/sh\necho \"GitHub Copilot 1.0.10.\"\n",
    )
    .unwrap();
    start_in_plain_folder(cx, window, data.path());
    wait_until(cx, |cx| {
        label(cx, window, "program-version-copilot") == "1.0.10"
    });
    assert_eq!(label(cx, window, "program-version-codex"), "0.16.0");
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_custom_command_label_starts_on_the_program_names(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    let (name_x, _, _, _) = bounds(cx, window, "program-name-codex").unwrap();
    let (label_x, _, _, _) = bounds(cx, window, "custom-command-text").unwrap();
    assert_close(label_x, name_x, "the custom command label");
}

#[cfg(unix)]
#[gpui_kit::test]
fn reopening_the_screen_closes_the_custom_command(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["codex"]);
    let project = start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    click(cx, window, &format!("project-new-{}", project.0));
    wait_until(cx, |cx| shows(cx, window, "program-search"));
    assert!(!shows(cx, window, "custom-program"));
}

/// Whether the row sits fully inside the program list's visible rows.
#[cfg(unix)]
fn in_program_list(cx: &mut TestAppContext, window: AnyWindowHandle, row: &str) -> bool {
    let (Some((_, y, _, height)), Some((_, list_y, _, list_height))) =
        (bounds(cx, window, row), bounds(cx, window, "program-list"))
    else {
        return false;
    };
    y >= list_y && y + height <= list_y + list_height
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_selected_program_below_the_cap_is_in_view_on_every_open(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &ROW_NAMES);
    let shell = bin.join("fixture-shell").display().to_string();
    let run = Run {
        project_id: ProjectId("personal".into()),
        program: shell.clone(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    cx.foreground_executor().block_test(started).unwrap();
    cx.run_until_parked();
    let project = start_in_plain_folder(cx, window, data.path());
    let row = format!("program-{shell}");
    wait_until(cx, |cx| in_program_list(cx, window, &row));
    // The person scrolls the list up, then leaves and comes back.
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::scroll_within(
            window,
            &ElementId::Name("program-list".into()),
            None,
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(2000.))),
            cx,
        )
        .unwrap();
    })
    .unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "escape");
    click(cx, window, &format!("project-new-{}", project.0));
    wait_until(cx, |cx| in_program_list(cx, window, &row));
}

#[cfg(unix)]
#[gpui_kit::test]
fn programs_show_while_versions_are_loading_and_keep_the_choice(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["codex", "gemini"]);
    let started = data.path().join("version-started");
    let release = data.path().join("version-release");
    std::fs::write(
        bin.join("codex"),
        format!(
            "#!/bin/sh\n/usr/bin/touch '{}'\nwhile [ ! -f '{}' ]; do /bin/sleep 0.05; done\necho codex-version\n",
            started.display(),
            release.display()
        ),
    )
    .unwrap();
    start_in_plain_folder(cx, window, data.path());
    wait_until(cx, |cx| shows(cx, window, "program-codex"));
    wait_until(cx, |_| started.exists());
    assert_eq!(selected_of(cx, window, "program-codex"), Some(true));
    cx.simulate_keystrokes(window, "down");
    wait_until(cx, |cx| {
        selected_of(cx, window, "program-gemini") == Some(true)
    });
    std::fs::write(&release, "").unwrap();
    wait_until(cx, |cx| {
        selected_of(cx, window, "program-gemini") == Some(true)
    });
    assert_eq!(selected_of(cx, window, "program-codex"), Some(false));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_selected_program_row_reports_selected_as_the_arrow_keys_move(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["codex"]);
    start_in_plain_folder(cx, window, data.path());
    let shell_row = format!("program-{}", bin.join("fixture-shell").display());
    assert_eq!(selected_of(cx, window, "program-codex"), Some(true));
    assert_eq!(selected_of(cx, window, &shell_row), Some(false));

    cx.simulate_keystrokes(window, "down");
    wait_until(cx, |cx| selected_of(cx, window, &shell_row) == Some(true));
    assert_eq!(selected_of(cx, window, "program-codex"), Some(false));
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_program_and_arguments_fields_report_which_one_has_focus(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["gemini"]);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    let program_before = value_of(cx, window, "custom-program");

    click(cx, window, "custom-program");
    assert_eq!(focused_of(cx, window, "custom-program"), Some(true));
    assert_eq!(focused_of(cx, window, "custom-arguments"), Some(false));

    click(cx, window, "custom-arguments");
    assert_eq!(focused_of(cx, window, "custom-program"), Some(false));
    assert_eq!(focused_of(cx, window, "custom-arguments"), Some(true));
    cx.simulate_input(window, "--fast");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "custom-arguments"),
        Some("--fast".into())
    );
    assert_eq!(value_of(cx, window, "custom-program"), program_before);
}

#[cfg(unix)]
#[gpui_kit::test]
fn the_search_box_filters_the_programs_as_you_type(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["codex", "gemini"]);
    start_in_plain_folder(cx, window, data.path());
    click(cx, window, "program-search");
    assert_eq!(focused_of(cx, window, "program-search"), Some(true));

    cx.simulate_input(window, "gem");
    wait_until(cx, |cx| {
        value_of(cx, window, "program-search") == Some("gem".into())
    });
    assert!(shows(cx, window, "program-gemini"), "the match stays");
    assert!(
        !shows(cx, window, "program-codex"),
        "the rest are filtered out"
    );
    assert_eq!(selected_of(cx, window, "program-gemini"), Some(true));

    cx.simulate_keystrokes(window, "backspace backspace backspace");
    wait_until(cx, |cx| shows(cx, window, "program-codex"));
    assert_eq!(value_of(cx, window, "program-search"), Some(String::new()));
}

#[cfg(unix)]
#[gpui_kit::test]
fn retry_lists_the_programs_once_the_shell_is_back(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let bin = fixture_programs(cx, data.path(), &["codex"]);
    // The shell is missing, so the programs cannot be listed.
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(&bin, &bin.join("no-shell"));
        });
    });
    let folder = data.path().join("plain");
    std::fs::create_dir_all(&folder).unwrap();
    let project = add_project(cx, &folder);
    click(cx, window, &format!("project-new-{}", project.id.0));
    wait_until(cx, |cx| shows(cx, window, "programs-retry"));
    assert_eq!(label(cx, window, "programs-retry"), "Retry");
    assert_eq!(role_of(cx, window, "programs-retry"), Some(Role::Button));

    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(&bin, &bin.join("fixture-shell"));
        });
    });
    click(cx, window, "programs-retry");
    wait_until(cx, |cx| shows(cx, window, "program-codex"));
    assert!(
        !shows(cx, window, "programs-retry"),
        "the failure goes away"
    );
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_in_the_search_starts_the_selected_program_not_the_custom_command(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    let _bin = fixture_programs(cx, data.path(), &["codex", "gemini"]);
    let project = start_in_plain_folder(cx, window, data.path());
    click(cx, window, "custom-command-toggle");
    wait_until(cx, |cx| shows(cx, window, "custom-program"));
    click(cx, window, "custom-program");
    cx.simulate_input(window, "custom-tool");
    cx.run_until_parked();

    // Back to the search: pick gemini there, and press Enter.
    click(cx, window, "program-search");
    cx.simulate_input(window, "gem");
    wait_until(cx, |cx| {
        value_of(cx, window, "program-search") == Some("gem".into())
    });
    cx.simulate_keystrokes(window, "enter");
    wait_until(cx, |cx| shown(cx, &kagan).is_some());
    assert_eq!(
        threads_in(cx, &project).len(),
        1,
        "exactly one thread starts"
    );
    let started = shown(cx, &kagan).unwrap();
    assert_eq!(thread_record(cx, &started).provider, "gemini");
}

#[cfg(unix)]
#[gpui_kit::test]
fn an_installed_agent_is_a_card_with_its_letter_mark_and_a_missing_one_has_none(
    cx: &mut TestAppContext,
) {
    let (_kagan, window, data) = open(cx);
    fixture_programs(cx, data.path(), &["claude"]);
    start_in_plain_folder(cx, window, data.path());
    wait_until(cx, |cx| {
        label(cx, window, "program-version-claude") == "1.2.3"
    });
    assert!(shows(cx, window, "program-mark-Claude Code"));
    // Codex is listed as a row while it is missing, so it has no card and no mark.
    assert!(!shows(cx, window, "program-mark-Codex"));
    assert!(!shows(cx, window, "program-mark-Shell"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_recent_command_is_named_by_its_program_with_its_flags_under_it(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let data = data.path().to_path_buf();
    fixture_programs(cx, &data, &["codex"]);
    let run = Run {
        project_id: ProjectId("personal".into()),
        program: "codex --model fast".into(),
        title: None,
        environment: EnvironmentSpec::Personal,
        visibility: None,
        parent_thread_id: None,
        quiet: false,
        lifecycle_owner_thread_id: None,
        metadata: None,
    };
    let started = cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Run(run)), cx));
    cx.foreground_executor().block_test(started).unwrap();
    cx.run_until_parked();
    start_in_plain_folder(cx, window, &data);
    wait_until(cx, |cx| {
        label(cx, window, "program-command-codex --model fast") == "--model fast"
    });
    // The default row is selected, so its name says so in words.
    assert_eq!(
        label(cx, window, "program-name-codex --model fast"),
        "Codex, selected"
    );
}
