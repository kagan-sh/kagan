#[cfg(unix)]
use std::path::Path;

use gpui_kit::TestAppContext;
#[cfg(unix)]
use gpui_kit::{AppContext as _, ElementId};
#[cfg(unix)]
use kg_core::Workspace;
#[cfg(unix)]
use kg_domain::{Op, PluginOp, ThreadId};

use super::wide;
#[cfg(unix)]
use super::{run, tab_to};
use crate::tests::kagan::{click, label, open, wait_until};
#[cfg(unix)]
use crate::tests::kagan::{press_key, shows, thread_record};

#[cfg(unix)]
#[gpui_kit::test]
fn add_plugin_starts_a_thread_and_pastes_without_enter(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(
        cx,
        data.path(),
        "printf 'Ready> '\nread line\nprintf 'SUBMITTED:%s' \"$line\"\nsleep 30",
    );
    assert_eq!(label(cx, window, "plugins-add"), "New plugin…");
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| {
        crate::tests::kagan::grid_text(cx, &kagan).contains("Build a kagan plugin that")
    });
    let text = crate::tests::kagan::grid_text(cx, &kagan);
    assert!(!text.contains("SUBMITTED:"));
    let id = cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap();
    let thread = thread_record(cx, &id);
    assert_eq!(thread.title, "New plugin");
    assert_eq!(thread.project_id.0, "personal");
}

#[cfg(unix)]
#[gpui_kit::test]
fn add_plugin_offers_copy_when_output_never_settles(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    let (kagan, window, data) = open(cx);
    plugin_agent(
        cx,
        data.path(),
        "i=0; while :; do printf '%s\\n' \"$i\"; i=$((i + 1)); sleep 0.1; done",
    );
    click(cx, window, "plugins-add");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !shows(cx, window, "plugin-prompt-notice") {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "no copy notice");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!crate::tests::kagan::grid_text(cx, &kagan).contains("Build a kagan plugin that"));
}

#[cfg(unix)]
pub(in crate::tests) fn plugin_agent(cx: &mut TestAppContext, data: &Path, script: &str) {
    use std::os::unix::fs::PermissionsExt as _;
    let bin = data.join("agent-bin");
    std::fs::create_dir_all(&bin).unwrap();
    let program = bin.join("claude");
    std::fs::write(&program, format!("#!/bin/sh\n{script}\n")).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    use_path(cx, &bin);
}

/// Finds programs in `bin` only, so the agents on this machine do not matter.
#[cfg(unix)]
pub(in crate::tests) fn use_path(cx: &mut TestAppContext, bin: &Path) {
    use_environment(cx, bin, Path::new("/bin/sh"));
}

#[cfg(unix)]
pub(in crate::tests) fn use_environment(cx: &mut TestAppContext, bin: &Path, shell: &Path) {
    let (bin, shell) = (bin.to_path_buf(), shell.to_path_buf());
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(&bin, &shell);
        });
    });
}

#[cfg(unix)]
pub(in crate::tests) fn wait_for_plugin(
    cx: &mut TestAppContext,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) -> ThreadId {
    wait_until(cx, |cx| {
        crate::tests::kagan::grid_text(cx, kagan).contains("Ready>")
    });
    cx.update(|cx| kagan.read(cx).shown_thread(cx)).unwrap()
}

#[cfg(unix)]
pub(in crate::tests) fn pump_for(cx: &mut TestAppContext, duration: std::time::Duration) {
    let until = std::time::Instant::now() + duration;
    while std::time::Instant::now() < until {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[cfg(unix)]
pub(in crate::tests) fn screen_text(cx: &mut TestAppContext, id: &ThreadId) -> String {
    let grid = cx
        .update(|cx| Workspace::global(cx).read(cx).grid(id, None))
        .unwrap();
    grid.lines.iter().flatten().map(|cell| cell.c).collect()
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_ignores_a_second_click_while_starting(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    cx.update_window(window, |_, window, cx| {
        let id = ElementId::Name("plugins-add".into());
        crate::ui_dispatch::click(window, &id, cx).unwrap();
        crate::ui_dispatch::click(window, &id, cx).unwrap();
    })
    .unwrap();
    wait_for_plugin(cx, &kagan);
    pump_for(cx, std::time::Duration::from_secs(2));
    let count = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread_records(cx)
            .iter()
            .filter(|t| t.title == "New plugin")
            .count()
    });
    assert_eq!(count, 1);
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_starts_the_agent_not_the_last_program(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    crate::tests::kagan::run_program(cx, "/bin/sh");
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    click(cx, window, "plugins-add");
    let id = wait_for_plugin(cx, &kagan);
    assert_eq!(thread_record(cx, &id).provider, "claude");
}

#[cfg(unix)]
#[gpui_kit::test]
fn new_plugin_says_so_when_no_agent_is_installed(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let empty = data.path().join("no-agents");
    std::fs::create_dir_all(&empty).unwrap();
    use_path(cx, &empty);
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| {
        label(cx, window, "note").contains("No agent is installed")
    });
    let started = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .thread_records(cx)
            .iter()
            .filter(|t| t.title == "New plugin")
            .count()
    });
    assert_eq!(started, 0);
}

#[gpui_kit::test]
fn selecting_a_project_targets_it_for_a_new_thread(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    wide(cx, window);
    click(cx, window, "project-prj_kagan");
    wait_until(cx, |cx| {
        label(cx, window, "start-prompt").starts_with("New thread in")
    });
    assert!(!label(cx, window, "start-prompt").contains("Personal"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn enter_on_a_project_starts_a_thread_there(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    wide(cx, window);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    tab_to(cx, window, "project-prj_kagan");
    press_key(cx, window, "enter");
    wait_until(cx, |cx| {
        let records = cx.update(|cx| Workspace::global(cx).read(cx).thread_records(cx));
        records
            .iter()
            .any(|t| t.project_id.0 == "prj_kagan" && t.provider == "claude")
    });
}

#[cfg(unix)]
#[gpui_kit::test]
fn typing_cancels_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    run(cx, Op::Plugin(PluginOp::Start)).unwrap();
    let id = wait_for_plugin(cx, &kagan);
    cx.simulate_keystrokes(window, "x");
    pump_for(cx, std::time::Duration::from_secs(2));
    let text = screen_text(cx, &id);
    assert!(text.contains('x'));
    assert!(!text.contains("Build a kagan plugin"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn switching_threads_cancels_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    let other = crate::tests::kagan::run_program(cx, "/bin/sh");
    click(cx, window, "plugins-add");
    let id = wait_for_plugin(cx, &kagan);
    click(cx, window, &format!("thread-{}", other.0));
    pump_for(cx, std::time::Duration::from_secs(2));
    assert!(!screen_text(cx, &id).contains("Build a kagan plugin"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn closing_the_window_cancels_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(cx, data.path(), "printf 'Ready> '; read line; sleep 30");
    click(cx, window, "plugins-add");
    let id = wait_for_plugin(cx, &kagan);
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    drop(kagan);
    pump_for(cx, std::time::Duration::from_secs(2));
    assert!(!screen_text(cx, &id).contains("Build a kagan plugin"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn a_paused_splash_can_receive_the_plugin_prompt(cx: &mut TestAppContext) {
    let (kagan, window, data) = open(cx);
    plugin_agent(
        cx,
        data.path(),
        "printf 'Ready> '; sleep 8; printf 'SECOND'; read line; sleep 30",
    );
    click(cx, window, "plugins-add");
    wait_until(cx, |cx| {
        crate::tests::kagan::grid_text(cx, &kagan).contains("Build a kagan plugin that")
    });
    assert!(!crate::tests::kagan::grid_text(cx, &kagan).contains("SECOND"));
}
