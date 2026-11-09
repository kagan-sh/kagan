use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
use gpui_kit::{App, KeyBinding, Menu, MenuItem, OsAction, Window, actions};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::WindowOp;

use super::window;
use super::{
    ActualSize, AddProject, ArchiveThread, Bigger, NewThread, OpenSettings, RenameThread, Smaller,
    StartAgain, StopThread, ToggleActivityLog, ToggleRightSidebar, ToggleSidebar,
};

actions!(
    kagan,
    [
        About, Hide, HideOthers, ShowAll, Quit, Minimize, Zoom, Close
    ]
);

pub(crate) fn bind(cx: &mut App) {
    super::navigation::bind(cx);
    super::tabs::bind(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
        KeyBinding::new("cmd-m", Minimize, None),
        KeyBinding::new("cmd-w", Close, None),
    ]);
    cx.bind_keys(crate::about::bindings());
    window::bind::<About>(cx, WindowOp::About);
    window::bind::<Hide>(cx, WindowOp::Hide);
    window::bind::<HideOthers>(cx, WindowOp::HideOthers);
    window::bind::<ShowAll>(cx, WindowOp::ShowAll);
    cx.on_action(|_: &Quit, cx| super::quit::ask(cx));
    super::quit::prepare(cx);
    window::bind::<Minimize>(cx, WindowOp::Minimize);
    window::bind::<Zoom>(cx, WindowOp::Zoom);
    window::bind::<Close>(cx, WindowOp::Close);
    window::bind::<ToggleActivityLog>(cx, WindowOp::ToggleActivityLog);
    let workspace = Workspace::global(cx);
    cx.subscribe(&workspace, |_, event, cx| {
        if let WorkspaceEvent::Window(op) = event {
            perform(*op, cx);
        }
    })
    .detach();
}

pub(crate) fn refresh(cx: &mut App) {
    cx.set_menus(menus());
}

fn perform(op: WindowOp, cx: &mut App) {
    match op {
        WindowOp::NextThread
        | WindowOp::PreviousThread
        | WindowOp::Thread { .. }
        | WindowOp::NextThreadNeedingYou => super::navigation::perform(op, cx),
        WindowOp::CommandList | WindowOp::SearchThreads => super::commands::perform(op, cx),
        WindowOp::NextTerminal | WindowOp::PreviousTerminal | WindowOp::OpenLocalLink => {
            super::tabs::perform(op, cx);
        }
        WindowOp::Quit => super::quit::finish(cx),
        WindowOp::Hide => cx.hide(),
        WindowOp::HideOthers => cx.hide_other_apps(),
        WindowOp::ShowAll => cx.unhide_other_apps(),
        WindowOp::About => crate::about::show(cx),
        WindowOp::Minimize => on_window(cx, |window| window.minimize_window()),
        WindowOp::Zoom => on_window(cx, |window| window.zoom_window()),
        WindowOp::Close => on_window(cx, Window::remove_window),
        WindowOp::ToggleActivityLog => window::with_front(cx, |this, _, cx| this.toggle_log(cx)),
        WindowOp::ToggleReviewWrap => window::with_front(cx, |this, _, cx| {
            if this.screen.review().is_some() {
                this.layout.review_wrap = !this.layout.review_wrap;
                super::layout::save(
                    &super::layout::key(kg_domain::prefs::REVIEW_WRAP),
                    this.layout.review_wrap,
                    cx,
                );
                cx.notify();
            }
        }),
    }
}

fn on_window(cx: &mut App, f: impl FnOnce(&mut Window)) {
    window::with_front(cx, |_, window, _| f(window));
}

fn menus() -> Vec<Menu> {
    vec![
        Menu {
            name: "kagan".into(),
            items: vec![
                MenuItem::action("About kagan", About),
                MenuItem::action("Check for Updates…", super::update::CheckForUpdates),
                MenuItem::Separator,
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::Separator,
                MenuItem::action("Hide Kagan", Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::Separator,
                MenuItem::action("Quit Kagan", Quit),
            ],
            disabled: false,
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New Thread", NewThread),
                MenuItem::action("Add Project…", AddProject),
                MenuItem::Separator,
                MenuItem::action("Close", Close),
            ],
            disabled: false,
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::os_action("Undo", Undo, OsAction::Undo),
                MenuItem::os_action("Redo", Redo, OsAction::Redo),
                MenuItem::Separator,
                MenuItem::os_action("Cut", Cut, OsAction::Cut),
                MenuItem::os_action("Copy", Copy, OsAction::Copy),
                MenuItem::os_action("Paste", Paste, OsAction::Paste),
                MenuItem::Separator,
                MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
            ],
            disabled: false,
        },
        Menu {
            name: "Thread".into(),
            items: vec![
                MenuItem::action("Stop", StopThread),
                MenuItem::action("Start Again", StartAgain),
                MenuItem::action("Rename…", RenameThread),
                MenuItem::action("Archive", ArchiveThread),
                MenuItem::Separator,
                MenuItem::action("Next Thread", super::navigation::NextThread),
                MenuItem::action("Previous Thread", super::navigation::PreviousThread),
                MenuItem::action(
                    "Next Thread Needing You",
                    super::navigation::NextThreadNeedingYou,
                ),
                MenuItem::action("Thread 1", super::navigation::Thread1),
                MenuItem::action("Thread 2", super::navigation::Thread2),
                MenuItem::action("Thread 3", super::navigation::Thread3),
                MenuItem::action("Thread 4", super::navigation::Thread4),
                MenuItem::action("Thread 5", super::navigation::Thread5),
                MenuItem::action("Thread 6", super::navigation::Thread6),
                MenuItem::action("Thread 7", super::navigation::Thread7),
                MenuItem::action("Thread 8", super::navigation::Thread8),
                MenuItem::action("Thread 9", super::navigation::Thread9),
            ],
            disabled: false,
        },
        Menu {
            name: "View".into(),
            items: vec![
                MenuItem::action("Bigger", Bigger),
                MenuItem::action("Smaller", Smaller),
                MenuItem::action("Actual Size", ActualSize),
                MenuItem::Separator,
                MenuItem::action("Left Sidebar", ToggleSidebar),
                MenuItem::action("Right Sidebar", ToggleRightSidebar),
            ],
            disabled: false,
        },
        Menu {
            name: "Window".into(),
            items: vec![
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::Separator,
                MenuItem::action("Close", Close),
            ],
            disabled: false,
        },
    ]
}
