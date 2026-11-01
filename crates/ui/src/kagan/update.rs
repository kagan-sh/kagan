//! The update notice, the install question, and Check for updates (decisions 113 to 115).

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window,
    actions,
};
use kg_core::{Updater, Workspace};
use kg_domain::{InstallKind, Op, OpResult, UpdateOp, UpdateState};

use super::{Kagan, Toast};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

actions!(kagan, [CheckForUpdates]);

pub(super) const BREW: &str = "brew upgrade --cask kagan";

pub(super) fn bind(cx: &mut App) {
    cx.on_action(|_: &CheckForUpdates, cx| check(cx));
}

pub(super) fn check(cx: &mut App) {
    if let Some((_, kagan)) = super::front(cx) {
        kagan.update(cx, |kagan, _| kagan.update_hidden = None);
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

fn say(message: String, cx: &mut App) {
    if let Some((_, kagan)) = super::front(cx) {
        kagan.update(cx, |kagan, cx| kagan.show_toast(Toast::said(message), cx));
    }
}

/// The quiet line over the center while a newer kagan waits.
pub(super) fn notice(kagan: &Kagan, cx: &mut Context<Kagan>) -> Option<AnyElement> {
    let updater = Updater::global(cx);
    let updater = updater.read(cx);
    let text = match updater.state() {
        UpdateState::Available { version, .. }
            if kagan.update_hidden.as_deref() != Some(version.as_str()) =>
        {
            match updater.kind() {
                InstallKind::Homebrew => format!("kagan {version} is available. Run {BREW}."),
                _ => format!("kagan {version} is available."),
            }
        }
        UpdateState::Downloading { version } => format!("Downloading kagan {version}…"),
        UpdateState::Ready { version, .. } => {
            format!("kagan {version} installs when kagan quits.")
        }
        _ => return None,
    };
    Some(
        h_flex()
            .id("update-notice")
            .aria_label(text.clone())
            .ui_container()
            .flex_wrap()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(cx.theme().border)
            .type_body()
            .child(text)
            .child(actions(cx))
            .when(
                matches!(updater.state(), UpdateState::Available { .. }),
                |row| {
                    row.child(button("update-hide", "Not now", |_, cx| {
                        if let Some((_, kagan)) = super::front(cx) {
                            kagan.update(cx, |kagan, cx| {
                                kagan.update_hidden = available(cx);
                                cx.notify();
                            });
                        }
                    }))
                },
            )
            .into_any_element(),
    )
}

/// What the person can do with the update the last check found.
fn actions(cx: &App) -> AnyElement {
    let updater = Updater::global(cx).read(cx);
    let notes = match updater.state() {
        UpdateState::Available { notes, .. } | UpdateState::Ready { notes, .. } => notes.clone(),
        _ => return h_flex().into_any_element(),
    };
    let ready = matches!(updater.state(), UpdateState::Ready { .. });
    h_flex()
        .flex_wrap()
        .gap_1()
        .child(button("update-notes", "Release notes", move |_, cx| {
            cx.open_url(&notes);
        }))
        .map(|row| match updater.kind() {
            InstallKind::Homebrew => row.child(button("update-copy", "Copy command", |_, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(BREW.into()));
                say("Copied.".into(), cx);
            })),
            kind if kind.replaces_itself() => row
                .when(!ready, |row| {
                    row.child(button(
                        "update-later",
                        "Install when kagan quits",
                        |_, cx| {
                            later(cx);
                        },
                    ))
                })
                .child(button(
                    "update-install",
                    "Restart and install…",
                    |window, cx| {
                        ask(window, cx);
                    },
                )),
            _ => row,
        })
        .into_any_element()
}

fn button(
    id: &'static str,
    label: &'static str,
    on_click: impl Fn(&mut Window, &mut App) + 'static,
) -> impl IntoElement {
    super::shown::control(
        id,
        Facts::button(label),
        Button::new(SharedString::from(format!("{id}-button")))
            .small()
            .outline()
            .label(label)
            .on_click(move |_, window, cx| on_click(window, cx)),
    )
}

fn available(cx: &App) -> Option<String> {
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
            cx.update(|cx| say(format!("Could not download the update: {error}"), cx));
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
    let description = match running {
        0 => "kagan quits, installs it and opens again.".to_owned(),
        1 => "kagan quits, installs it and opens again. 1 running program will end.".to_owned(),
        _ => format!(
            "kagan quits, installs it and opens again. {running} running programs will end."
        ),
    };
    window.open_alert_dialog(cx, move |dialog, _, _| {
        dialog
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
            updater.update(cx, |updater, cx| updater.replace(cx)).await
        }
        .await;
        cx.update(|cx| match installed {
            Ok(()) => super::quit::accept(cx),
            Err(error) => say(format!("Could not install the update: {error}"), cx),
        });
    })
    .detach();
}
