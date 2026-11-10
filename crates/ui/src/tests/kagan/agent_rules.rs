use std::path::{Path, PathBuf};

use gpui_kit::{AnyWindowHandle, TestAppContext};

use super::plugins::add_project;
use super::remove::right_click;
use super::{click, disabled, open, press_key, shows, wait_until};

/// Opens the rules dialog from a new project's row menu, then types into it without a click:
/// the dialog must take focus itself. Returns the project's rules file.
fn project_rules_dialog(cx: &mut TestAppContext, window: AnyWindowHandle, data: &Path) -> PathBuf {
    let folder = data.join("rules-project");
    std::fs::create_dir(&folder).unwrap();
    let project = add_project(cx, &folder);
    let row = format!("project-{}", project.id);
    wait_until(cx, |cx| shows(cx, window, &row));
    right_click(cx, window, &row);
    wait_until(cx, |cx| shows(cx, window, "project-agent-rules"));
    click(cx, window, "project-agent-rules");
    wait_until(cx, |cx| shows(cx, window, "agent-rules-text"));
    wait_until(cx, |cx| {
        disabled(cx, window, "agent-rules-save") == Some(false)
    });
    cx.simulate_input(window, "Use cargo t.");
    cx.run_until_parked();
    folder.join(".kagan").join("instructions.md")
}

fn saved_as(path: &Path, text: &str) -> bool {
    std::fs::read_to_string(path).ok().as_deref() == Some(text)
}

#[gpui_kit::test]
fn the_project_rules_dialog_saves_with_its_save_button(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let file = project_rules_dialog(cx, window, data.path());
    click(cx, window, "agent-rules-save");
    wait_until(cx, |_| saved_as(&file, "Use cargo t."));
}

#[gpui_kit::test]
fn command_enter_in_the_project_rules_dialog_saves(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let file = project_rules_dialog(cx, window, data.path());
    press_key(cx, window, "secondary-enter");
    wait_until(cx, |_| saved_as(&file, "Use cargo t."));
}
