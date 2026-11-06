use super::super::controls::{bound_keys, control};
use super::{FindInDiff, Kagan, NextChange, NextFile, PrevChange, PrevFile, Review, ToggleWrap};
use crate::ui_registry::Facts;
use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Selectable as _, Sizable as _, h_flex,
};
use gpui_kit::{
    Action, AnyElement, App, ClickEvent, Context, ElementId, FocusHandle, IntoElement,
    ParentElement as _, Styled as _, div,
};

impl Kagan {
    pub(super) fn review_actions(&self, review: &Review, cx: &mut Context<Self>) -> AnyElement {
        let viewed = self.is_viewed(&review.path, cx);
        let focus = &review.focus;
        h_flex()
            .flex_none()
            .items_center()
            .gap_2()
            .child(control(
                "review-viewed",
                Facts {
                    selected: Some(viewed),
                    ..Facts::button("Viewed")
                },
                Checkbox::new("checkbox-review-viewed")
                    .label("Viewed")
                    .checked(viewed)
                    .disabled(self.changes.viewed_checking.is_none())
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_viewed(cx);
                        this.focus_review(window, cx);
                    })),
            ))
            .child(preview_button(cx))
            .child(div().w_px().h_4().bg(cx.theme().border))
            .child(
                h_flex()
                    .child(tool(
                        "review-prev-file",
                        IconName::ChevronLeft,
                        "Previous file",
                        focus,
                        PrevFile,
                        cx,
                    ))
                    .child(tool(
                        "review-next-file",
                        IconName::ChevronRight,
                        "Next file",
                        focus,
                        NextFile,
                        cx,
                    )),
            )
            .child(
                h_flex()
                    .child(tool(
                        "review-previous",
                        IconName::ChevronUp,
                        "Previous change",
                        focus,
                        PrevChange,
                        cx,
                    ))
                    .child(tool(
                        "review-next",
                        IconName::ChevronDown,
                        "Next change",
                        focus,
                        NextChange,
                        cx,
                    )),
            )
            .child(tool(
                "review-find",
                IconName::Search,
                "Find",
                focus,
                FindInDiff,
                cx,
            ))
            .child(control(
                "review-wrap",
                Facts {
                    selected: Some(self.layout.review_wrap),
                    ..Facts::button("Wrap lines")
                },
                Button::new("button-review-wrap")
                    .ghost()
                    .small()
                    .icon(IconName::TextWrap)
                    .selected(self.layout.review_wrap)
                    .accessibility_label("Wrap lines")
                    .tooltip(format!("Wrap lines ({})", bound_keys(&ToggleWrap, cx)))
                    .on_click({
                        let focus = focus.clone();
                        move |_, window, cx| {
                            window.focus(&focus, cx);
                            window.dispatch_action(Box::new(ToggleWrap), cx);
                        }
                    }),
            ))
            .into_any_element()
    }
}

fn preview_button(cx: &mut Context<Kagan>) -> AnyElement {
    control(
        "review-preview",
        Facts::button("Preview"),
        Button::new("button-review-preview")
            .ghost()
            .small()
            .icon(IconName::Eye)
            .accessibility_label("Preview")
            .tooltip("Preview")
            .on_click(cx.listener(|this, _, window, cx| {
                this.preview_review(window, cx);
            })),
    )
    .into_any_element()
}

pub(super) fn tool<A: Action + Clone>(
    id: &'static str,
    icon: IconName,
    label: &'static str,
    focus: &FocusHandle,
    action: A,
    cx: &App,
) -> AnyElement {
    let keys = bound_keys(&action, cx);
    let tooltip = if keys.is_empty() {
        label.to_owned()
    } else {
        format!("{label} ({keys})")
    };
    let focus = focus.clone();
    control(
        id,
        Facts::button(label),
        Button::new(ElementId::Name(format!("button-{id}").into()))
            .ghost()
            .small()
            .icon(icon)
            .accessibility_label(label)
            .tooltip(tooltip)
            .on_click(move |event, window, cx| {
                if matches!(event, ClickEvent::Mouse(_)) {
                    window.focus(&focus, cx);
                }
                window.dispatch_action(action.boxed_clone(), cx);
            }),
    )
    .into_any_element()
}
