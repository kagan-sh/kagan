use crate::kagan::{Kagan, screen_read};
use std::cell::RefCell;
use std::rc::Rc;

use gpui_kit::Entity;
use gpui_kit::component::WindowExt as _;
use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, ClipboardItem, OsAction, OwnedMenu, OwnedMenuItem,
    TestAppContext,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{Op, SelectMode, ThreadOp, WindowOp};

use crate::kagan::menus::{About, Close, Hide, HideOthers, Minimize, Quit, ShowAll, Zoom};

use crate::tests::kagan::{
    chord, click, focused_of, grid_text, open, run_shell, shows, wait_for_output, wait_for_screen,
    wait_until,
};

#[gpui_kit::test]
fn the_menu_bar_lists_the_standard_commands(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open(cx);
    let menus = cx.update(|cx| cx.get_menus()).expect("menus");
    let names: Vec<_> = menus.iter().map(|menu| menu.name.to_string()).collect();
    assert_eq!(names, ["kagan", "File", "Edit", "Thread", "View", "Window"]);
    assert_app_menu(&menus[0]);
    assert_eq!(labels(&menus[1]), ["New Thread", "Add Project…", "Close"]);
    assert!(is_action::<crate::kagan::NewThread>(item(
        &menus[1],
        "New Thread"
    )));
    assert!(is_action::<crate::kagan::AddProject>(item(
        &menus[1],
        "Add Project…"
    )));
    assert!(is_action::<Close>(item(&menus[1], "Close")));
    assert_edit_menu(&menus[2]);
    assert_eq!(
        labels(&menus[3]),
        [
            "Stop",
            "Start Again",
            "Rename…",
            "Archive",
            "Next Thread",
            "Previous Thread",
            "Next Thread Needing You",
            "Thread 1",
            "Thread 2",
            "Thread 3",
            "Thread 4",
            "Thread 5",
            "Thread 6",
            "Thread 7",
            "Thread 8",
            "Thread 9",
        ]
    );
    assert_eq!(
        labels(&menus[4]),
        [
            "Bigger",
            "Smaller",
            "Actual Size",
            "Left Sidebar",
            "Right Sidebar"
        ]
    );
    assert_window_menu(&menus[5]);
}

fn assert_app_menu(menu: &OwnedMenu) {
    assert_eq!(
        labels(menu),
        [
            "About kagan",
            "Check for Updates…",
            "Settings…",
            "Hide Kagan",
            "Hide Others",
            "Show All",
            "Quit Kagan"
        ]
    );
    assert!(is_action::<About>(item(menu, "About kagan")));
    assert!(is_action::<Quit>(item(menu, "Quit Kagan")));
    assert_eq!(separators(menu), 3);
}

fn assert_edit_menu(menu: &OwnedMenu) {
    assert_eq!(
        labels(menu),
        ["Undo", "Redo", "Cut", "Copy", "Paste", "Select All"]
    );
    assert!(matches!(os(item(menu, "Undo")), Some(OsAction::Undo)));
    assert!(matches!(os(item(menu, "Redo")), Some(OsAction::Redo)));
    assert!(matches!(os(item(menu, "Cut")), Some(OsAction::Cut)));
    assert!(matches!(os(item(menu, "Copy")), Some(OsAction::Copy)));
    assert!(matches!(os(item(menu, "Paste")), Some(OsAction::Paste)));
    assert!(matches!(
        os(item(menu, "Select All")),
        Some(OsAction::SelectAll)
    ));
    assert!(is_action::<Undo>(item(menu, "Undo")));
    assert!(is_action::<Redo>(item(menu, "Redo")));
    assert!(is_action::<Cut>(item(menu, "Cut")));
    assert!(is_action::<Copy>(item(menu, "Copy")));
    assert!(is_action::<Paste>(item(menu, "Paste")));
    assert!(is_action::<SelectAll>(item(menu, "Select All")));
    assert_eq!(separators(menu), 2);
}

fn assert_window_menu(menu: &OwnedMenu) {
    assert_eq!(labels(menu), ["Minimize", "Zoom", "Close"]);
    assert!(is_action::<Minimize>(item(menu, "Minimize")));
    assert!(is_action::<Zoom>(item(menu, "Zoom")));
    assert!(is_action::<Close>(item(menu, "Close")));
    assert_eq!(separators(menu), 1);
}

#[gpui_kit::test]
fn command_keys_reach_the_window_while_the_terminal_is_focused(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let seen = record(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(terminal_focused(cx, window, &kagan));

    assert!(platform_key(cx, window, &Hide, "h", false));
    assert!(platform_key(cx, window, &HideOthers, "h", true));
    assert!(platform_key(cx, window, &Minimize, "m", false));
    assert!(platform_key(cx, window, &Quit, "q", false));
    assert!(platform_key(cx, window, &Close, "w", false));
    assert!(unbound(cx, window, &About));
    assert!(unbound(cx, window, &ShowAll));
    assert!(unbound(cx, window, &Zoom));

    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    cx.simulate_keystrokes(window, "escape");
    cx.run_until_parked();

    assert!(shows(cx, window, "project-prj_kagan"));
    cx.simulate_keystrokes(window, &chord("ToggleSidebar"));
    cx.run_until_parked();
    assert!(!shows(cx, window, "project-prj_kagan"));

    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window), "a running program did not ask");
    assert!(seen.borrow().is_empty());
}

#[gpui_kit::test]
fn command_w_closes_the_window_and_leaves_the_workspace(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let seen = record(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    assert!(terminal_focused(cx, window, &kagan));

    cx.simulate_keystrokes(window, "cmd-w");
    cx.run_until_parked();
    assert_eq!(seen.borrow().as_slice(), &[WindowOp::Close]);
    assert!(cx.update(|cx| cx.windows().is_empty()));
}

#[gpui_kit::test]
fn command_comma_and_quit_work_outside_the_terminal(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let seen = record(cx);
    cx.simulate_keystrokes(window, &chord("OpenSettings"));
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));

    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(!dialog(cx, window), "nothing running still asked");
    assert_eq!(crate::kagan::quit::quit_requests(), before + 1);
    assert!(seen.borrow().is_empty());
}

#[gpui_kit::test]
fn quit_is_the_focused_default_of_the_question(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    run_shell(cx);
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    assert_eq!(focused_of(cx, window, "quit-confirm"), Some(true));
    assert!(shows(cx, window, "quit-cancel"));
}

#[gpui_kit::test]
fn quit_asks_only_when_a_program_is_running(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(!dialog(cx, window));
    assert_eq!(crate::kagan::quit::quit_requests(), before + 1);

    let (_kagan, _first, _data) = open(cx);
    let id = run_shell(cx);
    let window = cx.update(|cx| *cx.windows().last().unwrap());
    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    assert_eq!(crate::kagan::quit::quit_requests(), before);
    assert!(pid(cx, &id).is_some());
    assert!(!cx.update(|cx| cx.windows().is_empty()));
}

#[gpui_kit::test]
fn cancel_keeps_the_running_program(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    cx.dispatch_action(window, gpui_kit::base::actions::Cancel);
    cx.run_until_parked();
    assert!(!dialog(cx, window));
    assert!(pid(cx, &id).is_some());
    assert!(!cx.update(|cx| cx.windows().is_empty()));
    assert_eq!(crate::kagan::quit::quit_requests(), before);
}

#[gpui_kit::test]
fn confirming_quit_ends_the_program(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    let Some(program) = pid(cx, &id) else {
        panic!("the shell did not start");
    };
    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, "enter");
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(!dialog(cx, window));
    wait_until(cx, |_| crate::kagan::quit::quit_requests() > before);
    assert!(pid(cx, &id).is_none());
    assert!(gone(program), "{program} survived quit");
    assert_eq!(crate::kagan::quit::quit_requests(), before + 1);
}

#[gpui_kit::test]
fn quit_finishes_when_the_screen_save_never_completes(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let id = run_shell(cx);
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| workspace.hold_screen_save(hold));
    });
    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    cx.dispatch_action(
        window,
        gpui_kit::base::actions::Confirm { secondary: false },
    );
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    let started = std::time::Instant::now();
    while pid(cx, &id).is_some() {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(8),
            "the shell did not end"
        );
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    cx.executor()
        .advance_clock(std::time::Duration::from_secs(4));
    cx.run_until_parked();
    wait_until(cx, |_| crate::kagan::quit::quit_requests() > before);
    assert_eq!(crate::kagan::quit::quit_requests(), before + 1);
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    assert!(
        store
            .screens()
            .unwrap()
            .iter()
            .all(|row| row.thread_id != id)
    );
    drop(release);
}

#[gpui_kit::test]
fn closing_the_window_during_the_question_asks_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));

    cx.simulate_keystrokes(window, "cmd-w");
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.windows().is_empty()));
    assert!(pid(cx, &id).is_some());

    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    let window = cx.update(|cx| cx.windows()[0]);
    let before = crate::kagan::quit::quit_requests();
    cx.simulate_keystrokes(window, "cmd-q");
    cx.run_until_parked();
    assert!(dialog(cx, window));
    assert_eq!(crate::kagan::quit::quit_requests(), before);
    assert!(pid(cx, &id).is_some());
}

#[gpui_kit::test]
fn the_socket_quit_does_not_ask(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    let before = crate::kagan::quit::quit_requests();
    cx.update(|cx| Workspace::dispatch(Op::Window(WindowOp::Quit), cx).detach());
    cx.run_until_parked();
    assert!(!dialog(cx, window), "kg window quit opened a dialog");
    assert_eq!(crate::kagan::quit::quit_requests(), before + 1);
    assert!(
        pid(cx, &id).is_some(),
        "kg window quit ended the program itself"
    );
}

#[gpui_kit::test]
fn closing_the_window_and_reopening_shows_it_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    cx.simulate_keystrokes(window, "cmd-w");
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.windows().is_empty()));

    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    cx.update(|cx| crate::open_window(cx).unwrap());
    cx.run_until_parked();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

// The last row that holds `word` whole, and its text.
fn last_row_with(
    cx: &mut TestAppContext,
    kagan: &Entity<Kagan>,
    word: &str,
) -> Option<(usize, String)> {
    let grid = cx.update(|cx| screen_read::terminal_grid(kagan.read(cx), cx))?;
    grid.lines.iter().enumerate().rev().find_map(|(row, line)| {
        let text: String = line.iter().map(|cell| cell.c).collect();
        text.contains(word).then_some((row, text))
    })
}

#[gpui_kit::test]
fn the_edit_menu_copy_and_paste_reach_the_terminal(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    cx.write_to_clipboard(ClipboardItem::new_string("menu-paste".into()));
    cx.dispatch_action(window, Paste);
    wait_for_screen(cx, &kagan, |cx| {
        grid_text(cx, &kagan).contains("menu-paste")
    });

    // Submit the pasted text, so the echo below starts on a clean prompt line.
    cx.simulate_keystrokes(window, "enter");
    cx.simulate_input(window, "echo menu-copy");
    cx.simulate_keystrokes(window, "enter");
    // The output is a row of its own. The typed command row can hold the word too, or wrap it
    // apart, so wait for a row that holds the whole word.
    wait_for_output(cx, &kagan, |cx| {
        last_row_with(cx, &kagan, "menu-copy").is_some()
    });
    let (row, text) = last_row_with(cx, &kagan, "menu-copy").unwrap();
    let column = text.find("menu-copy").unwrap() + 1;
    cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Select {
                id: id.clone(),
                mode: SelectMode::Word,
                column: u16::try_from(column).unwrap(),
                row: u16::try_from(row).unwrap(),
                right: false,
                terminal: None,
            }),
            cx,
        )
    })
    .detach();
    cx.run_until_parked();
    cx.dispatch_action(window, Copy);
    wait_until(cx, |cx| {
        cx.update(|cx| cx.read_from_clipboard())
            .and_then(|item| item.text())
            .is_some_and(|text| text == "menu-copy")
    });
}

fn record(cx: &mut TestAppContext) -> Rc<RefCell<Vec<WindowOp>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let record = seen.clone();
    cx.update(|cx| {
        let workspace = Workspace::global(cx);
        cx.subscribe(&workspace, move |_, event, _| {
            if let WorkspaceEvent::Window(op) = event {
                record.borrow_mut().push(*op);
            }
        })
        .detach();
    });
    seen
}

fn terminal_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_focused(kagan.read(cx), window, cx)
    })
    .unwrap()
}

fn platform_key(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    action: &dyn gpui_kit::Action,
    key: &str,
    alt: bool,
) -> bool {
    cx.update_window(window, |_, window, _| {
        window.bindings_for_action(action).iter().any(|binding| {
            let strokes = binding.keystrokes();
            strokes.len() == 1
                && strokes[0].key() == key
                && strokes[0].modifiers().platform
                && strokes[0].modifiers().alt == alt
                && !strokes[0].modifiers().shift
                && !strokes[0].modifiers().control
        })
    })
    .unwrap()
}

fn unbound(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    action: &dyn gpui_kit::Action,
) -> bool {
    cx.update_window(window, |_, window, _| {
        window.bindings_for_action(action).is_empty()
    })
    .unwrap()
}

fn item<'a>(menu: &'a OwnedMenu, name: &str) -> &'a OwnedMenuItem {
    menu.items
        .iter()
        .find(|item| matches!(item, OwnedMenuItem::Action { name: item_name, .. } if item_name == name))
        .unwrap_or_else(|| panic!("missing {name}"))
}

fn labels(menu: &OwnedMenu) -> Vec<&str> {
    menu.items
        .iter()
        .filter_map(|item| match item {
            OwnedMenuItem::Action { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect()
}

fn separators(menu: &OwnedMenu) -> usize {
    menu.items
        .iter()
        .filter(|item| matches!(item, OwnedMenuItem::Separator))
        .count()
}

fn os(item: &OwnedMenuItem) -> Option<OsAction> {
    match item {
        OwnedMenuItem::Action { os_action, .. } => *os_action,
        _ => None,
    }
}

fn is_action<T: gpui_kit::Action>(item: &OwnedMenuItem) -> bool {
    match item {
        OwnedMenuItem::Action { action, .. } => action.as_any().is::<T>(),
        _ => false,
    }
}

fn dialog(cx: &mut TestAppContext, window: AnyWindowHandle) -> bool {
    cx.update_window(window, |_, window, cx| window.has_active_dialog(cx))
        .unwrap()
}

fn pid(cx: &mut TestAppContext, id: &kg_domain::ThreadId) -> Option<u32> {
    cx.update(|cx| Workspace::global(cx).read(cx).terminal_process_id(id))
}

fn gone(pid: u32) -> bool {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while alive(pid) {
        if std::time::Instant::now() > deadline {
            return false;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    true
}

fn alive(pid: u32) -> bool {
    #[cfg(unix)]
    {
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }
    #[cfg(windows)]
    {
        std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH"])
            .output()
            .is_ok_and(|output| {
                String::from_utf8_lossy(&output.stdout)
                    .split_whitespace()
                    .any(|word| word == pid.to_string())
            })
    }
}
