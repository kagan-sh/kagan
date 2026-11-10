use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use kg_domain::{Skill, SkillSource};

use super::{Page, Sort};
use crate::kagan::controls::{control, note};
use crate::theme::{SpaceStep, TypeScale as _, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::scroll::ScrollableElement as _;

pub(super) fn render(page: &Page, window: &mut Window, cx: &mut Context<Page>) -> AnyElement {
    let skills = page.visible(cx);
    let body = if page.listing.is_none() {
        note(
            "skills-loading",
            "Loading skills…",
            cx.theme().muted_foreground,
            cx,
        )
    } else if page.listing.as_ref().is_some_and(Vec::is_empty) {
        note(
            "skills-empty",
            "No skills yet. Make one with New skill.",
            cx.theme().muted_foreground,
            cx,
        )
    } else if skills.is_empty() {
        note(
            "skills-none-match",
            "No skills match. Clear the search or the filter.",
            cx.theme().muted_foreground,
            cx,
        )
    } else {
        v_flex()
            .id("skills-list")
            .w_full()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .children(
                skills
                    .iter()
                    .map(|skill| row(skill, window, cx).into_any_element()),
            )
            .into_any_element()
    };
    v_flex()
        .overflow_y_scrollbar()
        .id("skills-page")
        .size_full()
        .min_h_0()
        .p(gpui_kit::px(space(SpaceStep::Lg, cx)))
        .gap(gpui_kit::px(space(SpaceStep::Md, cx)))
        .child(header(cx))
        .child(toolbar(page, cx))
        .when_some(page.creating.clone(), |this, input| {
            this.child(create_row(&input, cx))
        })
        .when_some(page.notice.clone(), |this, notice| {
            this.child(note("skills-notice", notice, cx.theme().danger, cx))
        })
        .children(page.errors.iter().enumerate().map(|(index, error)| {
            div()
                .id(("skills-error", index))
                .type_small(cx)
                .text_color(cx.theme().danger)
                .child(format!("Skipped: {error}"))
        }))
        .child(body)
        .into_any_element()
}

fn header(cx: &Context<Page>) -> impl IntoElement {
    v_flex()
        .gap(gpui_kit::px(space(SpaceStep::Xs, cx)))
        .child(
            div()
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child("Skills / My skills"),
        )
        .child(
            div()
                .type_body(cx)
                .text_color(cx.theme().muted_foreground)
                .child(
                    "The skills kagan gives every agent: built into kagan, from plugins, and the \
                     ones you wrote. A thread gets this same list.",
                ),
        )
}

fn toolbar(page: &Page, cx: &mut Context<Page>) -> impl IntoElement {
    let filter = filter_label(page.filter);
    let sort = match page.sort {
        Sort::Name => "Name",
        Sort::Source => "Source",
    };
    let next_filter = next_filter(page.filter);
    let next_sort = match page.sort {
        Sort::Name => Sort::Source,
        Sort::Source => Sort::Name,
    };
    h_flex()
        .gap(gpui_kit::px(space(SpaceStep::Sm, cx)))
        .child(div().flex_1().min_w_0().child(Input::new(&page.query)))
        .child(control(
            "skills-filter",
            Facts::button(format!("Filter: {filter}")),
            Button::new("skills-filter-button")
                .small()
                .outline()
                .label(format!("Filter: {filter}"))
                .on_click(cx.listener(move |this, _, _, cx| this.set_filter(next_filter, cx))),
        ))
        .child(control(
            "skills-sort",
            Facts::button(format!("Sort: {sort}")),
            Button::new("skills-sort-button")
                .small()
                .outline()
                .label(format!("Sort: {sort}"))
                .on_click(cx.listener(move |this, _, _, cx| this.set_sort(next_sort, cx))),
        ))
        .child(control(
            "skills-new",
            Facts::button("New skill"),
            Button::new("skills-new-button")
                .small()
                .primary()
                .label("New skill")
                .on_click(cx.listener(|this, _, window, cx| this.start_new(window, cx))),
        ))
}

fn next_filter(filter: Option<SkillSource>) -> Option<SkillSource> {
    match filter {
        None => Some(SkillSource::Bundled),
        Some(SkillSource::Bundled) => Some(SkillSource::Plugin),
        Some(SkillSource::Plugin) => Some(SkillSource::User),
        Some(SkillSource::User) => Some(SkillSource::Project),
        Some(SkillSource::Project) => None,
    }
}

fn filter_label(filter: Option<SkillSource>) -> &'static str {
    match filter {
        None => "All",
        Some(source) => source_label(source),
    }
}

pub(super) fn source_label(source: SkillSource) -> &'static str {
    match source {
        SkillSource::Bundled => "Built in",
        SkillSource::Plugin => "Plugin",
        SkillSource::User => "Yours",
        SkillSource::Project => "Project",
    }
}

fn source_icon(source: SkillSource) -> IconName {
    match source {
        SkillSource::Bundled => IconName::Cpu,
        SkillSource::Plugin => IconName::Frame,
        SkillSource::User => IconName::FileText,
        SkillSource::Project => IconName::FolderOpen,
    }
}

fn create_row(
    input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
    cx: &mut Context<Page>,
) -> impl IntoElement {
    h_flex()
        .gap(gpui_kit::px(space(SpaceStep::Sm, cx)))
        .child(div().flex_1().min_w_0().child(Input::new(input)))
        .child(control(
            "skills-create",
            Facts::button("Create skill"),
            Button::new("skills-create-button")
                .small()
                .primary()
                .label("Create skill")
                .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
        ))
}

fn row(skill: &Skill, window: &mut Window, cx: &mut Context<Page>) -> impl IntoElement {
    let name = skill.name.clone();
    let opened = name.clone();
    let focus = crate::kagan::tab_stop(
        window,
        cx,
        gpui_kit::ElementId::Name(format!("skill-focus-{}", skill.name).into()),
    );
    let included = matches!(skill.source, SkillSource::Plugin | SkillSource::Bundled);
    h_flex()
        .id(gpui_kit::ElementId::Name(
            format!("skill-row-{}", skill.name).into(),
        ))
        .key_context(super::ROW_CONTEXT)
        .track_focus(&focus)
        .focus_visible(|line| line.border_1().border_color(cx.theme().ring))
        .w_full()
        .px(gpui_kit::px(space(SpaceStep::Md, cx)))
        .py(gpui_kit::px(space(SpaceStep::Sm, cx)))
        .gap(gpui_kit::px(space(SpaceStep::Md, cx)))
        .items_center()
        .cursor_pointer()
        .border_b_1()
        .border_color(cx.theme().border)
        .hover(|row| row.bg(cx.theme().list_hover))
        .ui_observe_as(Facts {
            role: Role::Button,
            focus: Some(focus.clone()),
            ..Facts::button(skill.name.clone())
        })
        .on_action(cx.listener(move |this, _: &super::OpenRow, _, cx| {
            this.open(opened.clone(), cx);
        }))
        .on_click(cx.listener(move |this, _, _, cx| this.open(name.clone(), cx)))
        .child(Icon::new(source_icon(skill.source)).small())
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(gpui_kit::px(space(SpaceStep::Xs, cx)))
                .child(
                    h_flex()
                        .gap(gpui_kit::px(space(SpaceStep::Sm, cx)))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .type_body(cx)
                                .child(skill.name.clone()),
                        )
                        .child(
                            div()
                                .type_caption(cx)
                                .text_color(cx.theme().muted_foreground)
                                .child(source_label(skill.source)),
                        )
                        .when(included, |this| {
                            this.child(
                                div()
                                    .type_caption(cx)
                                    .px(gpui_kit::px(space(SpaceStep::Xs, cx)))
                                    .rounded(cx.theme().radius)
                                    .border_1()
                                    .border_color(cx.theme().border)
                                    .child("Included"),
                            )
                        }),
                )
                .child(
                    div()
                        .type_small(cx)
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(cx.theme().muted_foreground)
                        .child(skill.description.clone()),
                ),
        )
        .child(Icon::new(IconName::ChevronRight).small())
}
