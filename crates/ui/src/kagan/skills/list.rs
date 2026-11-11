use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, StatefulInteractiveElement as _, Styled as _, Window, div,
};
use kg_domain::{Skill, SkillSource, SkillTemplate};

use super::{Page, Save, Sort};
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
    } else if skills.is_empty() && page.query_is_empty(cx) && page.yours_is_empty() {
        note(
            "skills-empty",
            "You have no skills yet. Make one with New skill.",
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
            this.child(create_row(page, &input, cx))
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
                .child("Skills"),
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

const FILTERS: [Option<SkillSource>; 5] = [
    None,
    Some(SkillSource::Bundled),
    Some(SkillSource::Plugin),
    Some(SkillSource::User),
    Some(SkillSource::Project),
];

const SORTS: [(Sort, bool); 4] = [
    (Sort::Name, false),
    (Sort::Name, true),
    (Sort::Source, false),
    (Sort::Source, true),
];

const TEMPLATES: [SkillTemplate; 4] = [
    SkillTemplate::Blank,
    SkillTemplate::ReviewPr,
    SkillTemplate::ReleaseNotes,
    SkillTemplate::DebugTest,
];

fn toolbar(page: &Page, cx: &mut Context<Page>) -> impl IntoElement {
    h_flex()
        .gap(gpui_kit::px(space(SpaceStep::Sm, cx)))
        .child(div().flex_1().min_w_0().child(Input::new(&page.query)))
        .child(filter_menu(page, cx))
        .child(sort_menu(page, cx))
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

fn filter_menu(page: &Page, cx: &mut Context<Page>) -> impl IntoElement {
    let label = format!("Filter: {}", filter_label(page.filter));
    let current = page.filter;
    let counts = FILTERS.map(|filter| page.count(filter));
    let weak = cx.entity().downgrade();
    control(
        "skills-filter",
        Facts::button(label.clone()),
        Button::new("skills-filter-button")
            .small()
            .outline()
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                FILTERS.iter().zip(counts).enumerate().fold(
                    menu,
                    |menu, (index, (&filter, count))| {
                        let weak = weak.clone();
                        menu.item(menu_item(
                            format!("skills-filter-{index}"),
                            format!("{} ({count})", filter_label(filter)),
                            current == filter,
                            false,
                            move |cx| _ = weak.update(cx, |page, cx| page.set_filter(filter, cx)),
                        ))
                    },
                )
            }),
    )
}

fn sort_menu(page: &Page, cx: &mut Context<Page>) -> impl IntoElement {
    let label = format!("Sort: {}", sort_label(page.sort, page.descending));
    let (current, descending) = (page.sort, page.descending);
    let weak = cx.entity().downgrade();
    control(
        "skills-sort",
        Facts::button(label.clone()),
        Button::new("skills-sort-button")
            .small()
            .outline()
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                SORTS
                    .iter()
                    .enumerate()
                    .fold(menu, |menu, (index, &(sort, desc))| {
                        let weak = weak.clone();
                        menu.item(menu_item(
                            format!("skills-sort-{index}"),
                            sort_label(sort, desc).to_owned(),
                            current == sort && descending == desc,
                            false,
                            move |cx| _ = weak.update(cx, |page, cx| page.set_sort(sort, desc, cx)),
                        ))
                    })
            }),
    )
}

fn menu_item(
    id: String,
    text: String,
    selected: bool,
    disabled: bool,
    pick: impl Fn(&mut App) + 'static,
) -> PopupMenuItem {
    PopupMenuItem::element(move |_, _| {
        div()
            .id(ElementId::Name(id.clone().into()))
            .ui_observe_as(Facts {
                role: Role::MenuItem,
                selected: Some(selected),
                disabled,
                ..Facts::button(text.clone())
            })
            .child(text.clone())
    })
    .checked(selected)
    .disabled(disabled)
    .on_click(move |_, _, cx| pick(cx))
}

fn sort_label(sort: Sort, descending: bool) -> &'static str {
    match (sort, descending) {
        (Sort::Name, false) => "Name A–Z",
        (Sort::Name, true) => "Name Z–A",
        (Sort::Source, false) => "Source, built in first",
        (Sort::Source, true) => "Source, project first",
    }
}

fn template_label(template: SkillTemplate) -> &'static str {
    match template {
        SkillTemplate::Blank => "Blank",
        SkillTemplate::ReviewPr => "Review a pull request",
        SkillTemplate::ReleaseNotes => "Write release notes",
        SkillTemplate::DebugTest => "Debug a failing test",
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
    page: &Page,
    input: &gpui_kit::Entity<gpui_kit::component::input::InputState>,
    cx: &mut Context<Page>,
) -> impl IntoElement {
    let no_project = page.project.is_none();
    v_flex()
        .gap(gpui_kit::px(space(SpaceStep::Xs, cx)))
        .child(
            h_flex()
                .gap(gpui_kit::px(space(SpaceStep::Sm, cx)))
                .child(
                    div()
                        .id("skills-name")
                        .ui_observe_as(Facts::input("Skill name", input, cx))
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(input)),
                )
                .child(template_menu(page, cx))
                .child(scope_menu(page, cx))
                .child(control(
                    "skills-create",
                    Facts::button("Create skill"),
                    Button::new("skills-create-button")
                        .small()
                        .primary()
                        .label("Create skill")
                        .on_click(cx.listener(|this, _, _, cx| this.create(cx))),
                )),
        )
        .when(no_project, |column| {
            column.child(note(
                "skills-scope-reason",
                "Save in Project needs a project. Pick one in the project list.",
                cx.theme().muted_foreground,
                cx,
            ))
        })
}

fn template_menu(page: &Page, cx: &mut Context<Page>) -> impl IntoElement {
    let label = format!("Template: {}", template_label(page.template));
    let current = page.template;
    let weak = cx.entity().downgrade();
    control(
        "skills-template",
        Facts::button(label.clone()),
        Button::new("skills-template-button")
            .small()
            .outline()
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                TEMPLATES.iter().fold(menu, |menu, &template| {
                    let weak = weak.clone();
                    menu.item(menu_item(
                        format!("skills-template-{template:?}"),
                        template_label(template).to_owned(),
                        current == template,
                        false,
                        move |cx| _ = weak.update(cx, |page, cx| page.set_template(template, cx)),
                    ))
                })
            }),
    )
}

fn scope_menu(page: &Page, cx: &mut Context<Page>) -> impl IntoElement {
    let current = if page.project.is_some() {
        page.save
    } else {
        Save::Yours
    };
    let label = format!(
        "Save in: {}",
        match current {
            Save::Yours => "Yours",
            Save::Project => "Project",
        }
    );
    let has_project = page.project.is_some();
    let weak = cx.entity().downgrade();
    control(
        "skills-scope",
        Facts::button(label.clone()),
        Button::new("skills-scope-button")
            .small()
            .outline()
            .label(label)
            .dropdown_menu(move |menu, _, _| {
                [(Save::Yours, "Yours"), (Save::Project, "Project")]
                    .into_iter()
                    .fold(menu, |menu, (save, name)| {
                        let weak = weak.clone();
                        menu.item(menu_item(
                            format!("skills-scope-{name}"),
                            name.to_owned(),
                            current == save,
                            save == Save::Project && !has_project,
                            move |cx| _ = weak.update(cx, |page, cx| page.set_save(save, cx)),
                        ))
                    })
            }),
    )
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
