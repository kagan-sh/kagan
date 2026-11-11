use gpui_kit::component::WindowExt as _;
use gpui_kit::component::menu::{PopupMenu, PopupMenuItem};
use gpui_kit::{
    App, ClipboardItem, ElementId, Global, InteractiveElement as _, ParentElement as _, Role,
    Window, actions, div,
};
use kg_core::{Updater, Workspace};
use kg_domain::{InstallKind, Op, OpResult, UpdateOp, UpdateState};

use super::Toast;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

actions!(kagan, [CheckForUpdates]);

pub(super) const BREW: &str = "brew upgrade --cask kagan";

struct Hidden(String);
impl Global for Hidden {}

pub(super) fn bind(cx: &mut App) {
    cx.on_action(|_: &CheckForUpdates, cx| check(cx));
}

pub(super) fn check(cx: &mut App) {
    if cx.has_global::<Hidden>() {
        cx.remove_global::<Hidden>();
    }
    let task = Workspace::dispatch(Op::Update(UpdateOp::Check), cx);
    cx.spawn(async move |cx| {
        let message = match task.await {
            Ok(OpResult::Update(status)) if status.state == UpdateState::Current => {
                format!("kagan {} is the newest version.", status.version)
            }
            Err(error) => format!("Could not check for updates: {error}"),
            Ok(_) => return,
        };
        cx.update(|cx| say(message, cx));
    })
    .detach();
}

const COPY: &str = "Copy command";
const RESTART: &str = "Restart and install…";
const NOTES: &str = "Release notes";

// What this install's update menu adds to the notes: the copy command, or the restart.
#[derive(Clone, Copy)]
enum Primary {
    Copy,
    Restart,
    Notes,
}

impl Primary {
    fn of(kind: InstallKind) -> Self {
        match kind {
            InstallKind::Homebrew => Self::Copy,
            kind if kind.replaces_itself() => Self::Restart,
            _ => Self::Notes,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Copy => COPY,
            Self::Restart => RESTART,
            Self::Notes => NOTES,
        }
    }
}

fn copy_brew(cx: &mut App) {
    cx.write_to_clipboard(ClipboardItem::new_string(BREW.into()));
    say("Copied.".into(), cx);
}

fn say(message: String, cx: &mut App) {
    if let Some((_, kagan)) = super::front(cx) {
        kagan.update(cx, |kagan, cx| kagan.show_toast(Toast::said(message), cx));
    }
}

pub(super) fn progress_sentence(state: &UpdateState) -> Option<String> {
    match state {
        UpdateState::Downloading { version } => Some(format!("Downloading kagan {version}…")),
        UpdateState::Ready {
            version,
            at_quit: true,
            ..
        } => Some(format!("kagan {version} installs when kagan quits.")),
        UpdateState::Ready { version, .. } => Some(format!("kagan {version} is ready to install.")),
        _ => None,
    }
}

pub(super) fn hidden(cx: &App) -> Option<String> {
    cx.try_global::<Hidden>().map(|hidden| hidden.0.clone())
}

// The update actions, as the activity strip's Update menu offers them. Available and Ready
// both offer the notes and this install's install action.
pub(super) fn menu_items(menu: PopupMenu, cx: &App) -> PopupMenu {
    let updater = Updater::global(cx).read(cx);
    let (notes, at_quit, available) = match updater.state() {
        UpdateState::Available { notes, .. } => (notes.clone(), false, true),
        UpdateState::Ready { notes, at_quit, .. } => (notes.clone(), *at_quit, false),
        _ => return menu,
    };
    let menu = menu.item(item("update-notes", NOTES, move |_, cx| {
        cx.open_url(&notes);
    }));
    let menu = match Primary::of(updater.kind()) {
        Primary::Copy => menu.item(item("update-copy", Primary::Copy.label(), |_, cx| {
            copy_brew(cx);
        })),
        Primary::Restart => {
            let menu = if at_quit {
                menu
            } else {
                menu.item(item("update-later", "Install when kagan quits", |_, cx| {
                    later(cx);
                }))
            };
            menu.item(item(
                "update-install",
                Primary::Restart.label(),
                |window, cx| {
                    ask(window, cx);
                },
            ))
        }
        Primary::Notes => menu,
    };
    if !available {
        return menu;
    }
    menu.item(item("update-hide", "Not now", |_, cx| {
        if let Some(version) = available_version(cx) {
            cx.set_global(Hidden(version));
            cx.refresh_windows();
        }
    }))
}

fn item(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        div()
            .id(ElementId::Name(id.into()))
            .ui_observe_as(Facts {
                role: Role::MenuItem,
                ..Facts::button(label)
            })
            .child(label)
    })
    .on_click(move |_, window, cx| on_click(window, cx))
}

fn available_version(cx: &App) -> Option<String> {
    match Updater::global(cx).read(cx).state() {
        UpdateState::Available { version, .. } => Some(version.clone()),
        _ => None,
    }
}

fn later(cx: &mut App) {
    if crate::synthetic::refuse_for_person() {
        return;
    }
    let download = Updater::global(cx).update(cx, |updater, cx| updater.download(true, cx));
    cx.spawn(async move |cx| {
        if let Err(error) = download.await {
            cx.update(|cx| say(format!("Could not install the update: {error}"), cx));
        }
    })
    .detach();
}

fn ask(window: &mut Window, cx: &mut App) {
    if crate::synthetic::refuse_for_person() {
        return;
    }
    let version = match Updater::global(cx).read(cx).state() {
        UpdateState::Available { version, .. } | UpdateState::Ready { version, .. } => {
            version.clone()
        }
        _ => return,
    };
    let running = Workspace::global(cx).read(cx).running_programs(cx);
    let description = if running == 0 {
        "kagan quits, installs it and opens again.".to_owned()
    } else {
        format!(
            "kagan quits, installs it and opens again. {}",
            super::quit::consequence(running)
        )
    };
    window.open_alert_dialog(cx, move |dialog, _, _| {
        dialog
            .confirm()
            .title(format!("Install kagan {version}?"))
            .description(description.clone())
            .ok_text("Restart and install")
            .on_ok(|_, _, cx| {
                if !crate::synthetic::refuse_for_person() {
                    install(cx);
                }
                true
            })
    });
}

fn install(cx: &mut App) {
    let updater = Updater::global(cx);
    let download = updater.update(cx, |updater, cx| {
        if matches!(updater.state(), UpdateState::Ready { .. }) {
            gpui_kit::Task::ready(Ok(()))
        } else {
            updater.download(false, cx)
        }
    });
    cx.spawn(async move |cx| {
        let installed = async {
            download.await?;
            updater.update(cx, Updater::replace).await
        }
        .await;
        cx.update(|cx| match installed {
            Ok(()) => super::quit::accept(cx),
            Err(error) => say(format!("Could not install the update: {error}"), cx),
        });
    })
    .detach();
}

#[cfg(test)]
#[path = "update_test.rs"]
mod tests;
