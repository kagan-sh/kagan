use crate::theme::TypeScale as _;

use gpui_kit::component::{ActiveTheme as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, div, px,
};

use super::row;
use crate::theme::{SpaceStep, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;

pub(super) fn group_block(group: row::Group, cx: &App) -> AnyElement {
    let radius = cx.theme().radius;
    let line = cx.theme().border;
    let card = cx.theme().group_box;
    let muted = cx.theme().muted_foreground;
    let rows = group.rows.len();
    let anchored = group.anchored;
    let title = group.title.clone();
    let block = v_flex()
        .flex_shrink_0()
        .id(ElementId::Name(
            format!("settings-group-{}", group.key).into(),
        ))
        .ui_container()
        .gap(px(space(SpaceStep::Sm, cx)))
        .when(group.headed, |card| {
            card.child(
                div()
                    .id(ElementId::Name(
                        format!("settings-heading-{}", group.key).into(),
                    ))
                    .ui_container()
                    .aria_label(title.clone())
                    .type_caption(cx)
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(title),
            )
        })
        .child(
            v_flex()
                .rounded(radius)
                .border_1()
                .border_color(line)
                .bg(card)
                .overflow_hidden()
                .children(
                    group
                        .rows
                        .into_iter()
                        .enumerate()
                        .map(|(ix, row)| row_element(row, ix + 1 == rows, line, muted, cx)),
                ),
        )
        .into_any_element();
    if anchored {
        div()
            .flex_shrink_0()
            .id("settings-group-shortcuts")
            .ui_container()
            .child(block)
            .into_any_element()
    } else {
        block
    }
}

pub(super) fn row_element(
    row: row::Row,
    last: bool,
    line: Hsla,
    muted: Hsla,
    cx: &App,
) -> AnyElement {
    let show_text = !row.bare;
    let alias = row.alias;
    let note = row
        .note_content
        .unwrap_or_else(|| row.note.clone().into_any_element());
    // A floor, not `min_w_0`. `min_w_0` let a long choice shrink the title to one
    // letter a line. The choice wraps in the width that is left.
    let text = v_flex()
        .flex_1()
        .min_w(px(160.))
        .gap_0p5()
        .when(show_text && !row.title.is_empty(), |text| {
            let title = div()
                .id(ElementId::Name(format!("{}-title", row.id).into()))
                .ui_container()
                .type_body(cx)
                .child(row.title.clone());
            match alias {
                Some(alias) => text.child(
                    div()
                        .id(ElementId::Name(format!("settings-heading-{alias}").into()))
                        .ui_container()
                        .aria_label(row.title.clone())
                        .child(title)
                        .into_any_element(),
                ),
                None => text.child(title.into_any_element()),
            }
        })
        .when(!row.note.is_empty(), |text| {
            text.child(
                div()
                    .id(ElementId::Name(format!("{}-note", row.id).into()))
                    .ui_container()
                    .aria_label(row.note.clone())
                    .type_small(cx)
                    .text_color(muted)
                    .child(note),
            )
        });
    let frame = if row.wide {
        v_flex().gap(px(space(SpaceStep::Md, cx)))
    } else {
        h_flex()
            .gap(px(space(SpaceStep::Lg, cx)))
            .justify_between()
            .items_center()
    };
    let body = frame
        .when(
            show_text && (!row.title.is_empty() || !row.note.is_empty()),
            |frame| frame.child(text),
        )
        .child(if row.wide {
            row.control
        } else {
            h_flex()
                .min_w_0()
                .justify_end()
                .child(row.control)
                .into_any_element()
        });
    let marked = row.marked;
    let item = div()
        .id(row.id)
        .ui_observe_as(Facts {
            role: gpui_kit::Role::Group,
            name: row.title.clone(),
            value: None,
            disabled: false,
            selected: Some(marked),
            focus: None,
            hint: None,
        })
        .aria_label(row.title)
        .px(px(space(SpaceStep::Lg, cx)))
        .py(px(space(SpaceStep::Md, cx)))
        .when(!last, |item| item.border_b_1().border_color(line))
        .when(marked, |item| item.border_1().border_color(cx.theme().ring))
        .child(body)
        .into_any_element();
    match alias {
        Some(alias) => div()
            .id(ElementId::Name(format!("settings-group-{alias}").into()))
            .ui_container()
            .child(item)
            .into_any_element(),
        None => item,
    }
}
