use gpui_kit::component::Disableable as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, Role,
    Styled as _, div,
};
use kg_domain::SkillSource;

use super::list::source_label;
use super::{Page, Shown};
use crate::kagan::controls::{control, note};
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::scroll::ScrollableElement as _;

pub(super) fn render(page: &Page, cx: &mut Context<Page>) -> AnyElement {
    let Some(shown) = page.selected.as_ref() else {
        return div().into_any_element();
    };
    let editable = matches!(shown.skill.source, SkillSource::User | SkillSource::Project);
    v_flex()
        .overflow_y_scrollbar()
        .id("skills-detail")
        .size_full()
        .min_h_0()
        .p_4()
        .gap_3()
        .child(breadcrumb(&shown.skill.name, cx))
        .child(title(page, shown, editable, cx))
        .when_some(page.notice.clone(), |this, notice| {
            this.child(note("skills-detail-notice", notice, cx.theme().danger, cx))
        })
        .when_some(page.editor.clone(), |this, editor| {
            this.child(editor_view(&editor, cx))
        })
        .when(page.editor.is_none(), |this| {
            this.child(frontmatter(shown, cx))
                .child(body(shown, cx))
                .child(files(shown, cx))
        })
        .into_any_element()
}

fn breadcrumb(name: &str, cx: &Context<Page>) -> impl IntoElement {
    h_flex()
        .gap_1()
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .child("Skills /")
        .child(control(
            "skills-back",
            Facts::button("My skills"),
            Button::new("skills-back-button")
                .small()
                .ghost()
                .label("My skills")
                .on_click(cx.listener(|this, _, _, cx| this.back(cx))),
        ))
        .child(format!("> {name}"))
}

fn title(page: &Page, shown: &Shown, editable: bool, cx: &mut Context<Page>) -> impl IntoElement {
    let source_line = match shown.skill.path.as_ref() {
        Some(path) => format!("{} · {}", source_label(shown.skill.source), path.display()),
        None => source_label(shown.skill.source).to_owned(),
    };
    h_flex()
        .justify_between()
        .items_start()
        .gap_3()
        .child(
            v_flex()
                .min_w_0()
                .gap_1()
                .child(div().type_title(cx).child(shown.skill.name.clone()))
                .child(
                    div()
                        .type_small(cx)
                        .text_color(cx.theme().muted_foreground)
                        .child(source_line),
                ),
        )
        .child(actions(page, shown, editable, cx))
}

// Edit and Delete are for skills kagan owns. Other skills say why they are read-only, where
// they live, and what to do instead.
fn actions(page: &Page, shown: &Shown, editable: bool, cx: &mut Context<Page>) -> AnyElement {
    if page.confirming {
        return h_flex()
            .gap_2()
            .items_center()
            .child(note(
                "skills-confirm-text",
                "Delete this skill? Its folder and files are removed.",
                cx.theme().danger,
                cx,
            ))
            .child(control(
                "skills-confirm-cancel",
                Facts::button("Cancel"),
                Button::new("skills-confirm-cancel-button")
                    .small()
                    .outline()
                    .label("Cancel")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirming = false;
                        cx.notify();
                    })),
            ))
            .child(control(
                "skills-confirm-delete",
                Facts::button("Delete skill"),
                Button::new("skills-confirm-delete-button")
                    .small()
                    .danger()
                    .label("Delete skill")
                    .on_click(cx.listener(|this, _, _, cx| this.delete(cx))),
            ))
            .into_any_element();
    }
    let mut row = h_flex().gap_2().items_center();
    if editable {
        row = row
            .child(control(
                "skills-edit",
                Facts::button("Edit"),
                Button::new("skills-edit-button")
                    .small()
                    .outline()
                    .label("Edit")
                    .on_click(cx.listener(|this, _, window, cx| this.start_edit(window, cx))),
            ))
            .child(control(
                "skills-delete",
                Facts::button("Delete"),
                Button::new("skills-delete-button")
                    .small()
                    .danger()
                    .label("Delete")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.confirming = true;
                        cx.notify();
                    })),
            ));
    } else {
        let text = match shown.skill.source {
            SkillSource::Bundled => "Built into kagan, so it is read-only here.",
            _ => "Comes from a plugin, so it is read-only here.",
        };
        row = row.child(note(
            "skills-read-only",
            text,
            cx.theme().muted_foreground,
            cx,
        ));
    }
    let ready = page.project.is_some();
    row = row.child(control(
        "skills-fork",
        Facts::button("Fork"),
        Button::new("skills-fork-button")
            .small()
            .outline()
            .label("Fork")
            .disabled(!ready)
            .on_click(cx.listener(|this, _, _, cx| this.fork(cx))),
    ));
    if !ready {
        row = row.child(note(
            "skills-fork-reason",
            "Pick a project first. Fork opens a new thread in it.",
            cx.theme().muted_foreground,
            cx,
        ));
    }
    row.into_any_element()
}

fn editor_view(
    editor: &gpui_kit::Entity<gpui_kit::component::input::TextareaState>,
    cx: &mut Context<Page>,
) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(
            Textarea::new(editor)
                .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                .small(),
        )
        .child(
            h_flex()
                .gap_2()
                .child(control(
                    "skills-save",
                    Facts::button("Save skill"),
                    Button::new("skills-save-button")
                        .small()
                        .primary()
                        .label("Save skill")
                        .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                ))
                .child(control(
                    "skills-cancel-edit",
                    Facts::button("Cancel edit"),
                    Button::new("skills-cancel-edit-button")
                        .small()
                        .outline()
                        .label("Cancel")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.editor = None;
                            cx.notify();
                        })),
                )),
        )
}

// The frontmatter, as a key and value table.
fn frontmatter(shown: &Shown, cx: &Context<Page>) -> impl IntoElement {
    let mut rows = vec![
        ("name", shown.skill.name.clone()),
        ("description", shown.skill.description.clone()),
    ];
    if let Some(hint) = shown.skill.hint.clone() {
        rows.push(("argument-hint", hint));
    }
    v_flex()
        .gap_1()
        .child(div().type_small(cx).child("SKILL.md"))
        .child(
            v_flex()
                .border_1()
                .border_color(cx.theme().border)
                .rounded(cx.theme().radius)
                .children(rows.into_iter().map(|(key, value)| {
                    h_flex()
                        .gap_3()
                        .px_3()
                        .py_1()
                        .child(
                            div()
                                .w(gpui_kit::px(140.))
                                .type_small(cx)
                                .text_color(cx.theme().muted_foreground)
                                .child(key),
                        )
                        .child(
                            div()
                                .id(gpui_kit::ElementId::Name(
                                    format!("skills-field-{key}").into(),
                                ))
                                .flex_1()
                                .min_w_0()
                                .type_small(cx)
                                .ui_observe_as(Facts {
                                    role: Role::Status,
                                    ..Facts::button(value.clone())
                                })
                                .child(value),
                        )
                })),
        )
}

// The instructions, rendered as Markdown in a bordered box that scrolls.
fn body(shown: &Shown, cx: &Context<Page>) -> impl IntoElement {
    div()
        .overflow_y_scrollbar()
        .id("skills-body")
        .w_full()
        .max_h(gpui_kit::px(420.))
        .p_3()
        .border_1()
        .border_color(cx.theme().border)
        .rounded(cx.theme().radius)
        .child(TextView::markdown("skills-body-text", shown.body.clone()))
}

fn files(shown: &Shown, cx: &Context<Page>) -> impl IntoElement {
    let root = shown.skill.path.as_ref().and_then(|path| path.parent());
    let names: Vec<String> = shown
        .files
        .iter()
        .map(|file| match root {
            Some(root) => file
                .strip_prefix(root)
                .unwrap_or(file)
                .display()
                .to_string(),
            None => file.display().to_string(),
        })
        .collect();
    v_flex()
        .gap_1()
        .child(div().type_small(cx).child("Files"))
        .children(names.into_iter().enumerate().map(|(index, name)| {
            div()
                .id(("skills-file", index))
                .type_small(cx)
                .text_color(cx.theme().muted_foreground)
                .child(Icon::new(IconName::FileText).small())
                .child(name)
        }))
}
