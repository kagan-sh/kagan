#[cfg(test)]
pub(crate) mod read;
use crate::theme::TypeScale as _;
mod actions;
mod comment;
mod page;
mod search;
mod syntax;
mod text;
pub(super) mod tree;

pub(super) use comment::Drafts;

use text::renamed_from;

use std::collections::BTreeSet;

use gpui_kit::component::input::InputState;
use gpui_kit::component::{ActiveTheme as _, h_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, ParentElement as _, ScrollHandle, StatefulInteractiveElement as _, Styled as _,
    Subscription, Task, actions, div,
};
use kg_domain::{DiffScope, ReviewRow, ThreadId};

use super::Kagan;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(crate) const CONTEXT: &str = "Review";

actions!(
    review,
    [
        CloseReview,
        NextChange,
        PrevChange,
        FindInDiff,
        NextMatch,
        PrevMatch,
        NextFile,
        PrevFile,
        ToggleViewed,
        ToggleWrap,
        SendReview
    ]
);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane {
    File,
    Files,
}

pub(super) struct Review {
    pub(super) thread: ThreadId,
    pub(super) scope: DiffScope,
    pub(super) path: String,
    window: AnyWindowHandle,
    search: Entity<InputState>,
    _search_watch: Subscription,
    match_at: Option<usize>,
    match_row: Option<usize>,
    searching: Option<()>,
    selecting: Option<()>,
    focus: FocusHandle,
    renamed_from: Option<String>,
    notice: Option<String>,
    hunks: Vec<u32>,
    at: Option<usize>,
    // Unchanged text must not reset the editor's cursor during a poll.
    pub(super) shown: String,
    revision: String,
    pub(super) loading: bool,
    load: Task<()>,
    pub(super) generation: u64,
    pub(super) rows: Vec<ReviewRow>,
    syntax: syntax::Syntax,
    syntax_revision: String,
    expanded: BTreeSet<u32>,
    filter: Entity<InputState>,
    _filter_watch: Subscription,
    tree: bool,
    pane: Pane,
    collapsed: BTreeSet<String>,
    tree_focus: Option<String>,
    pub(super) draft: Option<usize>,
    drag: Option<(kg_domain::DiffSide, u32)>,
    scroll: ScrollHandle,
}

struct ReadExpect {
    generation: u64,
    thread: ThreadId,
    scope: DiffScope,
    path: String,
}

impl Review {
    pub(super) fn is_for(&self, thread: &ThreadId) -> bool {
        &self.thread == thread
    }

    pub(super) fn clear_syntax(&mut self) {
        self.syntax.clear();
        self.syntax_revision.clear();
    }
}

mod open;
mod render;
mod state;
fn change_position(review: &Review) -> String {
    match (review.hunks.len(), review.at) {
        (0, _) => "No line changes".to_owned(),
        (count, Some(at)) => format!("Change {} of {count}", at + 1),
        (count, None) => format!("{count} change{}", if count == 1 { "" } else { "s" }),
    }
}

fn review_title(review: &Review, cx: &mut Context<Kagan>) -> AnyElement {
    let (directory, name) = super::files::directory_and_name(&review.path);
    h_flex()
        .id("review-title")
        .ui_observe_as(Facts::button(review.path.clone()))
        .min_w_0()
        .whitespace_nowrap()
        .type_body(cx)
        .when_some(
            review
                .renamed_from
                .as_ref()
                .map(|old| format!("Renamed from {old}")),
            |title, renamed| {
                title.tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(renamed.clone()).build(window, cx)
                })
            },
        )
        .when(!directory.is_empty(), |title| {
            title.child(
                div()
                    .min_w_0()
                    .text_ellipsis_start()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{directory}/")),
            )
        })
        .child(
            div()
                .flex_none()
                .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                .child(name.to_owned()),
        )
        .into_any_element()
}
