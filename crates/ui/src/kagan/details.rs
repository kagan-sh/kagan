use crate::theme::TypeScale as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ClipboardItem, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Role, SharedString, Styled as _, Task, div,
};
use kg_core::Workspace;
use kg_domain::{EnvironmentProvider, GitDetails, Op, OpResult, ThreadDetails, ThreadId, ThreadOp};

use super::Kagan;
use super::changes::line;
use super::controls::control;
use super::right::Body;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

/// The shown thread's details, read through `ThreadOp::Details`.
pub(super) struct Details {
    thread: Option<ThreadId>,
    read: Option<Result<ThreadDetails, String>>,
    loading: bool,
    reload: bool,
    load: Task<()>,
}

impl Default for Details {
    fn default() -> Self {
        Self {
            thread: None,
            read: None,
            loading: false,
            reload: false,
            load: Task::ready(()),
        }
    }
}

impl Kagan {
    /// Whether the Details panel exists, so a timer should keep it current.
    pub(super) fn details_open(&self) -> bool {
        self.panels
            .iter()
            .any(|panel| matches!(panel.body, Body::Details))
    }

    pub(super) fn load_details(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.shown_thread(cx) else {
            self.details = Details::default();
            return;
        };
        if self.details.thread.as_ref() != Some(&thread) {
            self.details.thread = Some(thread.clone());
            self.details.read = None;
        }
        if self.details.loading {
            self.details.reload = true;
            return;
        }
        self.details.loading = true;
        let read = Workspace::dispatch(Op::Thread(ThreadOp::Details { id: thread.clone() }), cx);
        self.details.load = cx.spawn(async move |this, cx| {
            let read = read.await;
            _ = this.update(cx, |this, cx| {
                this.details.loading = false;
                if this.details.thread.as_ref() == Some(&thread) {
                    this.details.read = Some(match read {
                        Ok(OpResult::Details(details)) => Ok(details),
                        Ok(_) => Err("Could not read the details.".to_owned()),
                        Err(error) => Err(format!("Could not read the details: {error}")),
                    });
                }
                if std::mem::take(&mut this.details.reload) {
                    this.load_details(cx);
                }
                cx.notify();
            });
        });
    }

    pub(super) fn render_details(&self, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        match &self.details.read {
            None => line("details-loading", "Reading…", muted, cx).into_any_element(),
            Some(Err(error)) => {
                line("details-error", error.clone(), cx.theme().danger, cx).into_any_element()
            }
            Some(Ok(details)) => {
                let folder = details.folder.display().to_string();
                let kind = match details.environment {
                    EnvironmentProvider::Project => "Project folder",
                    EnvironmentProvider::Worktree => "Worktree",
                    EnvironmentProvider::Personal => "Personal folder",
                };
                v_flex()
                    .gap_2()
                    .child(
                        h_flex()
                            .gap_1()
                            .items_end()
                            .child(div().flex_1().min_w_0().child(fact(
                                "details-folder",
                                "Folder",
                                folder.clone(),
                                cx,
                            )))
                            .child(copy_folder(folder)),
                    )
                    .child(fact("details-environment", "Environment", kind.into(), cx))
                    .child(fact(
                        "details-program",
                        "Program",
                        details.program.clone(),
                        cx,
                    ))
                    .child(match &details.git {
                        Some(git) => git_facts(git, cx),
                        None => line("details-not-git", "Not a git folder.", muted, cx)
                            .into_any_element(),
                    })
                    .into_any_element()
            }
        }
    }
}

// `kg ui` reads the value from the element's facts.
fn fact(id: &'static str, label: &'static str, value: String, cx: &App) -> AnyElement {
    v_flex()
        .id(id)
        .ui_observe_as(Facts {
            role: Role::Label,
            value: Some(value.clone().into()),
            ..Facts::button(label)
        })
        .min_w_0()
        .child(
            div()
                .type_caption(cx)
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(div().type_body(cx).min_w_0().text_ellipsis().child(value))
        .into_any_element()
}

fn copy_folder(folder: String) -> impl IntoElement {
    control(
        "details-copy-folder",
        Facts::button("Copy folder"),
        Button::new("details-copy-folder-button")
            .xsmall()
            .ghost()
            .icon(IconName::Copy)
            .accessibility_label("Copy folder")
            .tooltip("Copy folder")
            .on_click(move |_, _, cx| {
                cx.write_to_clipboard(ClipboardItem::new_string(folder.clone()));
            }),
    )
}

fn base_text(git: &GitDetails) -> String {
    let Some(base) = &git.base else {
        return "None".to_owned();
    };
    match (base.ahead, base.behind) {
        (0, 0) => format!("{} · up to date", base.name),
        (ahead, 0) => format!("{} · {ahead} ahead", base.name),
        (0, behind) => format!("{} · {behind} behind", base.name),
        (ahead, behind) => format!("{} · {ahead} ahead, {behind} behind", base.name),
    }
}

fn git_facts(git: &GitDetails, cx: &App) -> AnyElement {
    let branch = git.branch.clone().unwrap_or_else(|| "Detached".to_owned());
    let mono = cx.theme().mono_font_family.clone();
    let commits = git.commits.iter().map(|commit| {
        let label = format!("{} {}", commit.rev, commit.summary);
        h_flex()
            .id(SharedString::from(format!("details-commit-{}", commit.rev)))
            .ui_observe_as(Facts {
                role: Role::Label,
                value: Some(label.clone().into()),
                ..Facts::button(label)
            })
            .gap_2()
            .min_w_0()
            .type_small(cx)
            .child(
                div()
                    .flex_none()
                    .font_family(mono.clone())
                    .text_color(cx.theme().muted_foreground)
                    .child(commit.rev.clone()),
            )
            .child(
                div()
                    .min_w_0()
                    .text_ellipsis()
                    .child(commit.summary.clone()),
            )
    });
    v_flex()
        .gap_2()
        .child(fact("details-branch", "Branch", branch, cx))
        .child(fact("details-base", "Base branch", base_text(git), cx))
        .child(
            v_flex()
                .id("details-commits")
                .ui_container()
                .gap_1()
                .child(
                    div()
                        .type_caption(cx)
                        .text_color(cx.theme().muted_foreground)
                        .child("Last commits"),
                )
                .children(commits),
        )
        .into_any_element()
}
