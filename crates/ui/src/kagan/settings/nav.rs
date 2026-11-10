use gpui_kit::Focusable as _;
use gpui_kit::assets::IconName;
use gpui_kit::component::input::Input;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, Icon, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, ElementId, InteractiveElement as _, IntoElement, ParentElement as _,
    Role, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, point, px, rems,
};
use kg_domain::PluginId;

use super::row::Group;
use super::{CONTEXT_SECTIONS, Kagan, NextSection, OpenSection, PreviousSection, pages};
use crate::kagan::controls::note;
use crate::theme::{SpaceStep, TypeScale as _, space};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

#[derive(Clone, PartialEq, Eq, Default)]
pub(super) enum Category {
    #[default]
    General,
    Appearance,
    Keyboard,
    Installed,
    Updates,
    Plugin(PluginId),
}

impl Category {
    fn key(&self) -> String {
        match self {
            Self::General => "general".into(),
            Self::Appearance => "appearance".into(),
            Self::Keyboard => "keyboard".into(),
            Self::Installed => "installed".into(),
            Self::Updates => "updates".into(),
            Self::Plugin(id) => format!("plugin-{}", id.0),
        }
    }
}

#[derive(Clone)]
pub(super) struct Listed {
    section: Category,
    label: SharedString,
    icon: IconName,
}

#[derive(Clone)]
pub(super) enum Entry {
    Category(Listed),
    Result {
        section: Category,
        group: SharedString,
        at: usize,
        title: SharedString,
        section_label: SharedString,
    },
}

const FIXED: [(Category, &str, IconName); 5] = [
    (Category::General, "General", IconName::Settings),
    (Category::Appearance, "Appearance", IconName::Palette),
    (Category::Keyboard, "Keyboard", IconName::Keyboard),
    (Category::Installed, "Installed", IconName::ListTree),
    (Category::Updates, "Updates", IconName::RotateCw),
];

fn listed(cx: &App) -> Vec<Listed> {
    FIXED
        .into_iter()
        .map(|(section, label, icon)| Listed {
            section,
            label: label.into(),
            icon,
        })
        .chain(
            pages::plugin_entries(cx)
                .into_iter()
                .map(|(id, name)| Listed {
                    section: Category::Plugin(id),
                    label: name.into(),
                    icon: IconName::Puzzle,
                }),
        )
        .collect()
}

pub(super) fn entries(kagan: &Kagan, groups: &[Group], cx: &App) -> Vec<Entry> {
    let query = kagan.settings.query.read(cx).value().trim().to_lowercase();
    let listed = listed(cx);
    if query.is_empty() {
        return listed.into_iter().map(Entry::Category).collect();
    }
    let mut out = Vec::new();
    for group in groups {
        let section_label = listed
            .iter()
            .find(|item| item.section == group.section)
            .map(|item| item.label.clone())
            .unwrap_or_default();
        // Questions is item 0 of the content column, so a group's position starts at 1.
        let at = groups
            .iter()
            .filter(|item| item.section == group.section)
            .position(|item| item.key == group.key)
            .map_or(0, |position| position + 1);
        for row in &group.rows {
            let hit = [&row.title, &row.note]
                .iter()
                .any(|text| text.to_lowercase().contains(&query));
            let title = if row.title.is_empty() {
                row.note.clone()
            } else {
                row.title.clone()
            };
            if hit && !title.is_empty() {
                out.push(Entry::Result {
                    section: group.section.clone(),
                    group: group.key.clone(),
                    at,
                    title,
                    section_label: section_label.clone(),
                });
            }
        }
    }
    out
}

pub(super) fn section_position(section: &Category, cx: &App) -> usize {
    listed(cx)
        .iter()
        .position(|item| item.section == *section)
        .unwrap_or(0)
}

impl Kagan {
    pub(super) fn select_section(
        &mut self,
        section: Category,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.cursor = section_position(&section, cx);
        self.settings.section = section;
        self.settings
            .query
            .update(cx, |query, cx| query.set_value("", window, cx));
        self.settings.scroll.set_offset(point(px(0.), px(0.)));
        cx.notify();
    }

    pub(super) fn settings_activate(
        &mut self,
        index: usize,
        entry: Entry,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings.cursor = index;
        match entry {
            Entry::Category(listed) => self.select_section(listed.section, window, cx),
            Entry::Result { section, at, .. } => {
                self.select_section(section, window, cx);
                let kagan = cx.entity();
                window.defer(cx, move |_, cx| {
                    kagan.update(cx, |this, cx| {
                        this.settings.scroll.scroll_to_top_of_item(at);
                        cx.notify();
                    });
                });
            }
        }
    }
}

pub(super) fn sidebar(
    kagan: &Kagan,
    entries: &[Entry],
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let focus = kagan.settings.list.clone();
    let focused = focus.is_focused(window);
    let len = entries.len();
    let cursor = kagan.settings.cursor.min(len.saturating_sub(1));
    let first_plugin = entries.iter().position(
        |entry| matches!(entry, Entry::Category(item) if matches!(item.section, Category::Plugin(_))),
    );
    let searching = !kagan.settings.query.read(cx).value().trim().is_empty();
    let open = entries.get(cursor).cloned();
    let mut rows: Vec<AnyElement> = Vec::new();
    if searching && entries.is_empty() {
        rows.push(note(
            "settings-search-none",
            "No settings match.",
            cx.theme().muted_foreground,
            cx,
        ));
    }
    for (index, entry) in entries.iter().enumerate() {
        if first_plugin == Some(index) {
            rows.push(
                div()
                    .px(px(space(SpaceStep::Sm, cx)))
                    .pt(px(space(SpaceStep::Md, cx)))
                    .type_caption(cx)
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child("Plugins")
                    .into_any_element(),
            );
        }
        let selected =
            matches!(entry, Entry::Category(item) if item.section == kagan.settings.section);
        rows.push(row(index, entry, selected, focused && index == cursor, cx));
    }
    let (line, radius) = (cx.theme().border, cx.theme().radius);
    let list = v_flex()
        .id("settings-sections")
        .track_focus(&focus)
        .ui_observe_as(Facts {
            focus: Some(focus.clone()),
            ..Facts::button("Settings sections")
        })
        .flex_1()
        .min_h_0()
        .w_full()
        .gap_0p5()
        .rounded(radius)
        .overflow_y_scrollbar()
        .on_action(cx.listener(move |this, _: &NextSection, _, cx| {
            this.settings.cursor = (cursor + 1).min(len.saturating_sub(1));
            cx.notify();
        }))
        .on_action(cx.listener(move |this, _: &PreviousSection, _, cx| {
            this.settings.cursor = cursor.saturating_sub(1);
            cx.notify();
        }))
        .children(rows);
    div()
        .w(rems(14.))
        .flex_shrink_0()
        .h_full()
        .p(px(space(SpaceStep::Sm, cx)))
        .border_r_1()
        .border_color(line)
        .child(
            v_flex()
                .size_full()
                .gap(px(space(SpaceStep::Sm, cx)))
                .key_context(CONTEXT_SECTIONS)
                .on_action(cx.listener(move |this, _: &OpenSection, window, cx| {
                    if let Some(entry) = open.clone() {
                        this.settings_activate(cursor, entry, window, cx);
                    }
                }))
                .child(
                    div()
                        .id("settings-search")
                        .ui_observe_as(Facts {
                            focus: Some(kagan.settings.query.read(cx).focus_handle(cx)),
                            ..Facts::button("Search settings")
                        })
                        .child(Input::new(&kagan.settings.query)),
                )
                .child(list),
        )
        .into_any_element()
}

fn row(
    index: usize,
    entry: &Entry,
    selected: bool,
    focused_here: bool,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let (ring, radius) = (cx.theme().ring, cx.theme().radius);
    let (hover, muted) = (cx.theme().muted, cx.theme().muted_foreground);
    let (id, label, content) = match entry {
        Entry::Category(item) => (
            format!("settings-section-{}", item.section.key()),
            item.label.clone(),
            h_flex()
                .w_full()
                .gap(px(space(SpaceStep::Sm, cx)))
                .px(px(space(SpaceStep::Sm, cx)))
                .py(px(space(SpaceStep::Xs, cx)))
                .items_center()
                .child(Icon::new(item.icon))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .truncate()
                        .type_body(cx)
                        .child(item.label.clone()),
                )
                .into_any_element(),
        ),
        Entry::Result {
            group,
            title,
            section_label,
            ..
        } => (
            format!("settings-result-{group}-{index}"),
            title.clone(),
            v_flex()
                .min_w_0()
                .px(px(space(SpaceStep::Sm, cx)))
                .py(px(space(SpaceStep::Xs, cx)))
                .child(div().truncate().type_body(cx).child(title.clone()))
                .child(
                    div()
                        .truncate()
                        .type_caption(cx)
                        .text_color(muted)
                        .child(section_label.clone()),
                )
                .into_any_element(),
        ),
    };
    let activate = entry.clone();
    div()
        .id(ElementId::Name(id.into()))
        .ui_observe_as(Facts {
            selected: Some(selected),
            ..Facts::button(label.clone())
        })
        .role(Role::Button)
        .aria_label(label)
        .w_full()
        .rounded(radius)
        .cursor_pointer()
        .when(selected, |row| {
            row.bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
        })
        .when(!selected, |row| row.hover(|row| row.bg(hover)))
        .when(focused_here, |row| row.border_1().border_color(ring))
        .on_click(cx.listener(move |this, _, window, cx| {
            this.settings_activate(index, activate.clone(), window, cx);
        }))
        .child(content)
        .into_any_element()
}
