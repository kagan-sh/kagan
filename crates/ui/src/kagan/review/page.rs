use super::{Kagan, renamed_from};
use crate::kagan::changes::{counts, files_label};
use crate::kagan::controls::Selected;
use crate::theme;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::{ActiveTheme as _, Icon, Sizable as _, h_flex, v_flex};

const LINE_FADE_WIDTH: gpui_kit::Pixels = gpui_kit::px(24.);
const LINE_FADE_HEIGHT: gpui_kit::Rems = gpui_kit::rems(1.125);
const GUTTER_TINT: f32 = 0.09;

pub(super) const GUTTER: gpui_kit::Pixels = gpui_kit::px(60.);
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};
use kg_core::Workspace;
use kg_domain::{DiffLineKind, DiffOp, DiffScope, Op, OpResult};

use super::rows::ReviewRow;

impl Kagan {
    pub(super) fn review_summary(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(review) = self.screen.review() else {
            return div().into_any_element();
        };
        let name = self
            .shown_project(cx)
            .and_then(|id| {
                self.sidebar
                    .read(cx)
                    .project(&id)
                    .map(|project| project.name.clone())
            })
            .unwrap_or_default();
        let added: u32 = self.changes.files.iter().map(|file| file.added).sum();
        let removed: u32 = self.changes.files.iter().map(|file| file.removed).sum();
        let count = self.changes.files.len();
        let viewed = self
            .changes
            .files
            .iter()
            .filter(|file| self.is_viewed(&file.path, cx))
            .count();
        let mut scopes = super::super::controls::segments(cx);
        for (scope, label) in [
            (DiffScope::Uncommitted, "Uncommitted"),
            (DiffScope::Branch, "Branch"),
            (DiffScope::Commit, "Last commit"),
        ] {
            let selected = review.scope == scope;
            scopes = scopes.child(super::super::controls::control(
                ElementId::Name(format!("review-scope-{label}").into()),
                Facts {
                    selected: Some(selected),
                    ..Facts::button(label)
                },
                super::super::controls::segment(
                    ElementId::Name(format!("button-review-scope-{label}").into()),
                    if selected {
                        Selected::Yes
                    } else {
                        Selected::No
                    },
                    cx,
                )
                .label(label)
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.set_scope(scope, cx);
                    this.focus_review(window, cx);
                })),
            ));
        }
        v_flex()
            .flex_none()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .h_11()
                    .gap_3()
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .type_body(cx)
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(div().flex_none().child(scopes))
                    .child(super::actions::tool(
                        "review-close",
                        gpui_kit::assets::IconName::X,
                        "Close",
                        &review.focus,
                        super::CloseReview,
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .pb_2()
                    .gap_2()
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(div().flex_none().child(files_label(count)))
                    .child(counts(added, removed, cx))
                    .child(
                        div()
                            .id("review-viewed-count")
                            .ui_container()
                            .aria_label(format!("{viewed} of {count} viewed"))
                            .child(format!("· {viewed} of {count} viewed")),
                    ),
            )
            .into_any_element()
    }

    fn viewed_key(&self, path: &str, cx: &Context<Self>) -> Option<String> {
        let project = self.shown_project(cx)?;
        let scope = self.screen.review()?.scope;
        Some(format!("{project}:{scope:?}:{path}"))
    }

    pub(super) fn is_viewed(&self, path: &str, cx: &Context<Self>) -> bool {
        self.viewed_key(path, cx)
            .is_some_and(|key| self.changes.viewed.contains_key(&key))
    }

    pub(super) fn invalidate_viewed(&mut self, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review() else {
            return;
        };
        let Some(key) = self.viewed_key(&review.path, cx) else {
            return;
        };
        if self
            .changes
            .viewed
            .get(&key)
            .is_some_and(|mark| mark != &review.revision)
        {
            self.changes.viewed.remove(&key);
            super::super::layout::save("review-viewed", &self.changes.viewed, cx);
        }
    }

    pub(in crate::kagan) fn validate_viewed(&mut self, cx: &mut Context<Self>) {
        if self.changes.viewed_checking != Some(false) {
            return;
        }
        let Some(review) = self.screen.review() else {
            return;
        };
        let thread = review.thread.clone();
        let scope = review.scope;
        let checks: Vec<_> = self
            .changes
            .files
            .iter()
            .filter_map(|file| {
                let key = self.viewed_key(&file.path, cx)?;
                let mark = self.changes.viewed.get(&key)?.clone();
                let task = Workspace::dispatch(
                    Op::Diff(DiffOp::Show {
                        query: super::super::changes::query(thread.clone(), scope),
                        path: file.path.clone(),
                    }),
                    cx,
                );
                Some((key, mark, task))
            })
            .collect();
        if checks.is_empty() {
            return;
        }
        self.changes.viewed_checking = Some(true);
        self.changes.viewed_check = cx.spawn(async move |this, cx| {
            let mut changed = Vec::new();
            for (key, mark, task) in checks {
                if let Ok(OpResult::DiffFile(view)) = task.await
                    && view.revision != mark
                {
                    changed.push((key, mark));
                }
            }
            _ = this.update(cx, |this, cx| {
                this.changes.viewed_checking = Some(false);
                let mut dirty = false;
                for (key, mark) in changed {
                    if this.changes.viewed.get(&key) == Some(&mark) {
                        this.changes.viewed.remove(&key);
                        dirty = true;
                    }
                }
                if dirty {
                    super::super::layout::save("review-viewed", &this.changes.viewed, cx);
                    cx.notify();
                }
            });
        });
    }

    pub(super) fn toggle_viewed(&mut self, cx: &mut Context<Self>) {
        if self.changes.viewed_checking.is_none() {
            return;
        }
        let Some(review) = self.screen.review() else {
            return;
        };
        let Some(key) = self.viewed_key(&review.path, cx) else {
            return;
        };
        if self.changes.viewed.remove(&key).is_none() {
            self.changes.viewed.insert(key, review.revision.clone());
        }
        super::super::layout::save("review-viewed", &self.changes.viewed, cx);
        cx.notify();
    }

    pub(super) fn review_file(&mut self, next: bool, cx: &mut Context<Self>) {
        let files = self.review_files(true, cx);
        let all = self.review_files(false, cx);
        let Some(index) = self
            .screen
            .review()
            .and_then(|review| all.iter().position(|path| *path == review.path))
        else {
            return;
        };
        let candidates: Box<dyn Iterator<Item = &String>> = if next {
            Box::new(all.iter().skip(index + 1))
        } else {
            Box::new(all[..index].iter().rev())
        };
        let Some(path) = candidates.into_iter().find(|path| files.contains(path)) else {
            return;
        };
        let path = path.clone();
        self.select_review_file(&path, cx);
    }

    pub(super) fn select_review_file(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let Some(file) = self.changes.files.iter().find(|file| file.path == path) else {
            return;
        };
        if review.path == file.path {
            review.pane = super::Pane::File;
            cx.notify();
            return;
        }
        let draft = self
            .review_drafts
            .active(&review.thread, review.scope, &file.path);
        review.pane = super::Pane::File;
        review.retarget(review.scope, file.path.clone(), renamed_from(file), draft);
        self.changes.selected = Some(file.path.clone());
        self.refresh_review(cx);
        cx.notify();
    }

    pub(super) fn review_row(
        &self,
        index: usize,
        row: &ReviewRow,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let fold_key = match row {
            ReviewRow::Fold { lines } => lines.first().and_then(|line| line.new.or(line.old)),
            ReviewRow::Line { .. } => None,
        };
        match row {
            ReviewRow::Fold { lines }
                if fold_key.is_some_and(|key| {
                    !self
                        .screen
                        .review()
                        .is_some_and(|review| review.expanded.contains(&key))
                }) =>
            {
                let key = fold_key.unwrap_or(0);
                h_flex()
                    .id(ElementId::Name(format!("review-fold-{key}").into()))
                    .ui_observe_as(Facts::button(format!("{} unmodified lines", lines.len())))
                    .w_full()
                    .h_6()
                    .items_center()
                    .bg(cx.theme().secondary.opacity(0.5))
                    .hover(|row| row.bg(cx.theme().secondary))
                    .type_small(cx)
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        h_flex().w(GUTTER).flex_none().justify_center().child(
                            Icon::new(gpui_kit::assets::IconName::ChevronsUpDown)
                                .xsmall()
                                .text_color(cx.theme().muted_foreground),
                        ),
                    )
                    .child(
                        div()
                            .pl_1()
                            .child(format!("{} unmodified lines", lines.len())),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(review) = this.screen.review_mut() {
                            review.expanded.insert(key);
                        }
                        this.focus_review(window, cx);
                        cx.notify();
                    }))
                    .into_any_element()
            }
            ReviewRow::Fold { lines } => v_flex()
                .children(
                    lines
                        .iter()
                        .map(|line| {
                            self.review_line(
                                line,
                                None,
                                self.screen.review().is_some_and(|review| {
                                    review.selecting.is_some() || review.match_row == Some(index)
                                }),
                                cx,
                            )
                        })
                        .collect::<Vec<_>>(),
                )
                .into_any_element(),
            ReviewRow::Line { line, changed } => self.review_line(
                line,
                changed.as_ref(),
                self.screen.review().is_some_and(|review| {
                    review.selecting.is_some() || review.match_row == Some(index)
                }),
                cx,
            ),
        }
    }

    fn review_line(
        &self,
        line: &kg_domain::DiffLine,
        changed: Option<&std::ops::Range<usize>>,
        selected: bool,
        cx: &mut Context<Kagan>,
    ) -> AnyElement {
        let color = match line.kind {
            DiffLineKind::Add => Some(theme::added(cx)),
            DiffLineKind::Remove => Some(cx.theme().danger),
            DiffLineKind::Context => None,
        };
        let background = color.map(|color| color.opacity(0.10));
        let (side, number) = match line.kind {
            DiffLineKind::Remove => (kg_domain::DiffSide::Old, line.old),
            DiffLineKind::Add | DiffLineKind::Context => (kg_domain::DiffSide::New, line.new),
        };
        let Some(number) = number else {
            return div().into_any_element();
        };
        let width = line_width(&line.text, cx);
        let wrap = self.layout.review_wrap;
        let mut row = h_flex()
            .when(!wrap, |row| row.min_w(width))
            .when(wrap, |row| row.w_full().min_w_0())
            .font_family(cx.theme().mono_font_family.clone())
            .group("review-line")
            .when(!wrap, |row| row.h(gpui_kit::rems(1.125)))
            .text_size(px(crate::theme::type_mono(cx)))
            .line_height(gpui_kit::rems(1.125))
            .id(ElementId::Name(
                format!("review-line-{side:?}-{number}").into(),
            ))
            .on_mouse_down(
                gpui_kit::MouseButton::Left,
                cx.listener(move |this, _, _, _| {
                    if let Some(review) = this.screen.review_mut() {
                        review.drag = Some((side, number));
                    }
                }),
            )
            .on_mouse_up(
                gpui_kit::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    let start = this
                        .screen
                        .review_mut()
                        .and_then(|review| review.drag.take());
                    if let Some((anchor_side, anchor)) = start
                        .filter(|(anchor_side, anchor)| *anchor_side == side && *anchor != number)
                    {
                        this.begin_comment(
                            anchor_side,
                            anchor.min(number),
                            anchor.max(number),
                            window,
                            cx,
                        );
                    }
                }),
            )
            .when(!wrap, gpui_kit::Styled::whitespace_nowrap)
            .items_start()
            .flex_shrink_0()
            .when_some(background, gpui_kit::Styled::bg)
            .when(selected, |row| row.border_1().border_color(cx.theme().ring));
        row = row.child(Self::review_gutter(side, number, color, cx));
        let spans = self.screen.review().map_or(Vec::new(), |review| {
            review.syntax.spans(side, number).to_vec()
        });
        let code = painted_line(&line.text, &spans, changed, color, wrap, cx);
        row = row.child(code);
        if wrap {
            return v_flex()
                .w_full()
                .min_w_0()
                .child(row)
                .child(self.inline_comments(side, number, cx))
                .into_any_element();
        }
        let fade = self.line_fade(width, cx);
        v_flex()
            .relative()
            .min_w(width)
            .child(row)
            .children(fade)
            .child(self.inline_comments(side, number, cx))
            .into_any_element()
    }
    fn line_fade(&self, width: gpui_kit::Pixels, cx: &Context<Kagan>) -> Option<AnyElement> {
        let edge = self
            .screen
            .review()
            .map(|review| review.scroll.bounds().size.width - review.scroll.offset().x)?;
        (edge > LINE_FADE_WIDTH && width > edge).then(|| {
            div()
                .absolute()
                .top_0()
                .left(edge - LINE_FADE_WIDTH)
                .w(LINE_FADE_WIDTH)
                .h(LINE_FADE_HEIGHT)
                .bg(gpui_kit::linear_gradient(
                    90.,
                    gpui_kit::linear_color_stop(cx.theme().background.opacity(0.), 0.),
                    gpui_kit::linear_color_stop(cx.theme().background, 1.),
                ))
                .into_any_element()
        })
    }
    fn review_gutter(
        side: kg_domain::DiffSide,
        number: u32,
        color: Option<gpui_kit::Hsla>,
        cx: &mut Context<Kagan>,
    ) -> AnyElement {
        h_flex()
            .id(ElementId::Name(format!("comment-{side:?}-{number}").into()))
            .ui_observe_as(Facts::button("Add comment"))
            .w(GUTTER)
            .h(gpui_kit::rems(1.125))
            .flex_shrink_0()
            .items_center()
            .pr_2()
            .text_color(color.unwrap_or(cx.theme().muted_foreground))
            .when_some(color, |gutter, color| {
                gutter
                    // Over the row's 0.10 tint, this reads as 0.18.
                    .bg(color.opacity(GUTTER_TINT))
                    .border_l_2()
                    .border_color(color)
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.begin_comment(side, number, number, window, cx);
            }))
            .child(
                h_flex()
                    .size_4()
                    .flex_none()
                    .justify_center()
                    .rounded(cx.theme().radius / 2.)
                    .bg(cx.theme().primary)
                    .invisible()
                    .group_hover("review-line", gpui_kit::Styled::visible)
                    .child(
                        Icon::new(gpui_kit::assets::IconName::Plus)
                            .xsmall()
                            .text_color(cx.theme().primary_foreground),
                    ),
            )
            .child(div().flex_1().text_right().child(number.to_string()))
            .into_any_element()
    }
}

fn painted_line(
    text: &str,
    spans: &[(std::ops::Range<usize>, String)],
    changed: Option<&std::ops::Range<usize>>,
    tint: Option<gpui_kit::Hsla>,
    wrap: bool,
    cx: &Context<Kagan>,
) -> AnyElement {
    let theme = cx.theme().highlight_theme.clone();
    div()
        .pl_1()
        .when(wrap, |line| line.flex_1().min_w_0().whitespace_normal())
        .when(!wrap, |line| line.flex_shrink_0().whitespace_nowrap())
        .child(super::syntax::line(
            text,
            spans,
            changed,
            tint,
            theme.as_ref(),
        ))
        .into_any_element()
}

fn line_width(text: &str, cx: &Context<Kagan>) -> gpui_kit::Pixels {
    let columns = text
        .chars()
        .map(|ch| {
            if ch == '\t' {
                4
            } else if ch.is_ascii() {
                1
            } else {
                2
            }
        })
        .sum::<usize>();
    let columns = u16::try_from(columns).unwrap_or(u16::MAX);
    f32::from(columns.saturating_add(8)) * cx.theme().mono_font_size * 0.8
}
