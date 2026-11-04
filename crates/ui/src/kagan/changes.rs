use crate::theme::TypeScale as _;
use std::collections::BTreeMap;
use std::time::Duration;

use gpui_kit::Role;
use gpui_kit::component::button::Button;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, FocusHandle, Hsla, InteractiveElement as _, IntoElement,
    KeyBinding, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Task, Window, actions, div,
};
use kg_core::Workspace;
use kg_domain::{DiffFile, DiffOp, DiffQuery, DiffScope, DiffStatus, Op, OpResult, ThreadId};

use super::Kagan;
use super::shown::control;
use crate::theme;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(crate) const POLL: Duration = Duration::from_secs(2);

actions!(changes, [ActivateChange]);

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("enter", ActivateChange, Some("ChangeRow")),
        KeyBinding::new("space", ActivateChange, Some("ChangeRow")),
    ]
}

pub(super) struct Changes {
    pub(super) thread: Option<ThreadId>,
    pub(super) scope: DiffScope,
    pub(super) files: Vec<DiffFile>,
    repository: bool,
    error: Option<String>,
    pub(super) selected: Option<String>,
    pub(super) viewed: BTreeMap<String, String>,
    pub(super) viewed_checking: Option<bool>,
    pub(super) viewed_check: Task<()>,
    // Reusing handles across list reloads preserves keyboard focus.
    focus: BTreeMap<String, FocusHandle>,
    loading: bool,
    reload: bool,
    load: Task<()>,
}

impl Changes {
    pub(super) fn new() -> Self {
        Self {
            thread: None,
            scope: DiffScope::Uncommitted,
            files: Vec::new(),
            repository: true,
            error: None,
            selected: None,
            viewed: BTreeMap::new(),
            viewed_checking: None,
            viewed_check: Task::ready(()),
            focus: BTreeMap::new(),
            loading: false,
            reload: false,
            load: Task::ready(()),
        }
    }

    #[cfg(test)]
    pub(super) fn focus_of(&self, path: &str) -> Option<&FocusHandle> {
        self.focus.get(path)
    }
}

pub(super) fn query(thread: ThreadId, scope: DiffScope) -> DiffQuery {
    DiffQuery {
        thread_id: thread,
        scope,
        base: None,
        rev: (scope == DiffScope::Commit).then(|| "HEAD".to_owned()),
    }
}

pub(super) fn line(
    id: &'static str,
    text: impl Into<SharedString>,
    color: Hsla,
) -> impl IntoElement {
    div()
        .id(id)
        .ui_container()
        .type_body()
        .text_color(color)
        .child(text.into())
}

pub(super) fn kind(status: DiffStatus) -> &'static str {
    match status {
        DiffStatus::Added => "A",
        DiffStatus::Modified => "M",
        DiffStatus::Deleted => "D",
        DiffStatus::Renamed => "R",
    }
}

fn count_text(count: u32, added: bool) -> String {
    if count == 0 {
        String::new()
    } else if added {
        format!("+{count}")
    } else {
        format!("−{count}")
    }
}

pub(super) fn files_label(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

pub(super) fn counts(added: u32, removed: u32, cx: &App) -> impl IntoElement {
    h_flex()
        .gap_1()
        .type_caption()
        .font_family(cx.theme().mono_font_family.clone())
        .when(added > 0, |line| {
            line.child(
                div()
                    .text_color(theme::added(cx))
                    .child(format!("+{added}")),
            )
        })
        .when(removed > 0, |line| {
            line.child(
                div()
                    .text_color(cx.theme().danger)
                    .child(format!("−{removed}")),
            )
        })
}

fn row_counts(file: &DiffFile) -> Option<SharedString> {
    let added = count_text(file.added, true);
    let removed = count_text(file.removed, false);
    match (added.is_empty(), removed.is_empty()) {
        (true, true) => None,
        (false, true) => Some(added.into()),
        (true, false) => Some(removed.into()),
        (false, false) => Some(format!("{added} {removed}").into()),
    }
}

pub(super) fn poll_open_changes(cx: &mut Context<Kagan>) -> Task<()> {
    cx.spawn(async move |this, cx| {
        loop {
            cx.background_executor().timer(POLL).await;
            let open = this.update(cx, |this, cx| {
                if this.right_mounted() || this.thread_visible(cx) {
                    this.load_changes(cx);
                    if this.details_open() {
                        this.load_details(cx);
                    }
                    this.load_local_link(cx);
                }
            });
            if open.is_err() {
                break;
            }
        }
    })
}

impl Kagan {
    pub(super) fn load_changes(&mut self, cx: &mut Context<Self>) {
        let Some(thread) = self.shown_thread(cx) else {
            self.changes = Changes::new();
            if self.screen.review().is_some() {
                self.screen.close();
            }
            return;
        };
        if self
            .screen
            .review()
            .is_some_and(|review| !review.is_for(&thread))
        {
            self.screen.close();
        }
        if self.changes.loading {
            self.changes.reload = true;
            return;
        }
        self.changes.loading = true;
        let scope = self.changes.scope;
        let listed = Workspace::dispatch(
            Op::Diff(DiffOp::List {
                query: query(thread.clone(), scope),
            }),
            cx,
        );
        self.changes.load = cx.spawn(async move |this, cx| {
            let listed = listed.await;
            _ = this.update(cx, |this, cx| {
                this.finish_list_read(thread, scope, listed, cx);
            });
        });
    }

    fn finish_list_read(
        &mut self,
        thread: ThreadId,
        scope: DiffScope,
        listed: Result<OpResult, kg_domain::OpError>,
        cx: &mut Context<Self>,
    ) {
        self.changes.loading = false;
        if self.shown_thread(cx).as_ref() == Some(&thread) && self.changes.scope == scope {
            self.apply_changes(thread, listed, cx);
        } else {
            self.changes.reload = true;
        }
        if std::mem::take(&mut self.changes.reload) {
            self.load_changes(cx);
        }
        cx.notify();
    }

    fn apply_changes(
        &mut self,
        thread: ThreadId,
        listed: Result<OpResult, kg_domain::OpError>,
        cx: &mut Context<Self>,
    ) {
        let (files, repository, error) = match listed {
            Ok(OpResult::Diff(list)) => (list.files, list.repository, None),
            Ok(_) => (
                Vec::new(),
                true,
                Some("Could not read the changes.".to_owned()),
            ),
            Err(error) => (
                Vec::new(),
                true,
                Some(format!("Could not read the changes: {error}")),
            ),
        };
        self.changes.thread = Some(thread);
        self.changes.files = files;
        self.changes.repository = repository;
        self.changes.error = error;
        for file in &self.changes.files {
            self.changes
                .focus
                .entry(file.path.clone())
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }
        self.sync_review_after_changes(cx);
        self.refresh_review(cx);
        self.validate_viewed(cx);
    }

    pub(super) fn set_scope(&mut self, scope: DiffScope, cx: &mut Context<Self>) {
        if self.changes.scope == scope {
            return;
        }
        self.changes.scope = scope;
        self.changes.files.clear();
        self.changes.selected = None;
        if let Some(review) = self.screen.review_mut() {
            review.scope = scope;
            review.draft = self
                .review_drafts
                .active(&review.thread, scope, &review.path);
            review.shown.clear();
            review.rows.clear();
            review.loading = false;
            review.generation = review.generation.wrapping_add(1);
        }
        self.load_changes(cx);
    }

    pub(super) fn render_changes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        if self.changes.thread.is_none() {
            return line(
                "changes-none",
                "Open a thread to see its changes.",
                cx.theme().muted_foreground,
            )
            .into_any_element();
        }
        v_flex()
            .gap_2()
            .when(self.changes.repository, |body| {
                body.child(self.scope_row(cx))
            })
            .child(self.changes_body(cx))
            .into_any_element()
    }

    fn scope_row(&self, cx: &mut Context<Self>) -> AnyElement {
        super::shown::segments(cx)
            .child(self.scope_button(
                DiffScope::Uncommitted,
                "Uncommitted",
                "changes-scope-uncommitted",
                cx,
            ))
            .child(self.scope_button(DiffScope::Branch, "Branch", "changes-scope-branch", cx))
            .into_any_element()
    }

    fn scope_button(
        &self,
        scope: DiffScope,
        label: &'static str,
        name: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let active = self.changes.scope == scope;
        control(
            name,
            Facts {
                selected: Some(active),
                ..Facts::button(label)
            },
            super::shown::segment(ElementId::Name(format!("button-{name}").into()), active, cx)
                .label(label)
                .on_click(cx.listener(move |this, _, _, cx| this.set_scope(scope, cx))),
        )
        .into_any_element()
    }

    fn changes_body(&self, cx: &mut Context<Self>) -> AnyElement {
        if let Some(error) = &self.changes.error {
            return line("changes-error", error.clone(), cx.theme().danger).into_any_element();
        }
        if !self.changes.repository {
            return line(
                "changes-not-git",
                "Not a git folder.",
                cx.theme().muted_foreground,
            )
            .into_any_element();
        }
        if self.changes.files.is_empty() {
            return line("changes-empty", "No changes.", cx.theme().muted_foreground)
                .into_any_element();
        }
        let added: u32 = self.changes.files.iter().map(|file| file.added).sum();
        let removed: u32 = self.changes.files.iter().map(|file| file.removed).sum();
        let summary = h_flex()
            .justify_between()
            .items_center()
            .child(
                h_flex()
                    .gap_2()
                    .child(
                        div()
                            .type_small()
                            .text_color(cx.theme().muted_foreground)
                            .child(files_label(self.changes.files.len())),
                    )
                    .child(counts(added, removed, cx)),
            )
            .child(control(
                "changes-open-review",
                Facts::button("Open review"),
                Button::new("button-changes-open-review")
                    .small()
                    .outline()
                    .label("Open review")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_selected_review(window, cx);
                    })),
            ));
        let rows: Vec<AnyElement> = self
            .changes
            .files
            .iter()
            .map(|file| self.change_row(file, cx))
            .collect();
        v_flex()
            .gap_1()
            .child(summary)
            .children(rows)
            .into_any_element()
    }

    fn change_row(&self, file: &DiffFile, cx: &mut Context<Self>) -> AnyElement {
        let path = file.path.clone();
        let selected = self.changes.selected.as_deref() == Some(file.path.as_str());
        let focus = self
            .changes
            .focus
            .get(&file.path)
            .cloned()
            .unwrap_or_else(|| cx.focus_handle().tab_stop(true));
        let (opened, activated) = (path.clone(), path.clone());
        div()
            .id(ElementId::Name(format!("change-{}", file.path).into()))
            .ui_observe_as(Facts {
                hint: None,
                role: Role::ListBoxOption,
                name: file.path.clone().into(),
                value: row_counts(file),
                disabled: false,
                selected: Some(selected),
                focus: Some(focus.clone()),
            })
            .track_focus(&focus)
            .tab_stop(true)
            .key_context("ChangeRow")
            .on_action(cx.listener(move |this, _: &ActivateChange, window, cx| {
                this.open_review(activated.clone(), window, cx);
            }))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.open_review(opened.clone(), window, cx);
            }))
            .h_7()
            .px_2()
            .gap_2()
            .flex()
            .items_center()
            .rounded(cx.theme().radius)
            .type_body()
            .when(selected, |row| row.bg(cx.theme().list_active))
            .hover(|row| row.bg(cx.theme().list_hover))
            .focus(|row| row.border_1().border_color(cx.theme().ring))
            .tooltip({
                let path = file.path.clone();
                move |window, cx| Tooltip::new(path.clone()).build(window, cx)
            })
            .child(
                div()
                    .w_3()
                    .type_caption()
                    .font_family(cx.theme().mono_font_family.clone())
                    .font_weight(gpui_kit::FontWeight::MEDIUM)
                    .text_color(cx.theme().muted_foreground)
                    .child(kind(file.status)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_ellipsis()
                    .child(file.path.clone()),
            )
            .child(counts(file.added, file.removed, cx))
            .into_any_element()
    }

    fn open_selected_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let listed = |path: &str| self.changes.files.iter().any(|file| file.path == path);
        let path = self
            .changes
            .selected
            .as_ref()
            .filter(|path| listed(path))
            .cloned()
            .or_else(|| self.changes.files.first().map(|file| file.path.clone()));
        if let Some(path) = path {
            self.open_review(path, window, cx);
        }
    }

    #[cfg(test)]
    pub(crate) fn change_focus(&self, path: &str) -> Option<FocusHandle> {
        self.changes.focus_of(path).cloned()
    }

    #[cfg(test)]
    pub(crate) fn finish_list_read_for_test(
        &mut self,
        thread: ThreadId,
        scope: DiffScope,
        listed: Result<OpResult, kg_domain::OpError>,
        cx: &mut Context<Self>,
    ) {
        self.finish_list_read(thread, scope, listed, cx);
    }
}
