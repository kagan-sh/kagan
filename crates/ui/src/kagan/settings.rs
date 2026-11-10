use crate::theme::{SpaceStep, TypeScale as _, control_height, radius_md, space};

mod indexes;
mod installed;
mod keyboard;
mod nav;
mod pages;
mod plugins;
pub(super) mod project;
mod row;
mod slots;
mod updates;

use crate::ui_registry::Facts;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, FocusHandle, Hsla,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, ScrollHandle,
    StatefulInteractiveElement as _, Styled as _, Subscription, Window, actions, div, px, rems,
};
use kg_core::{Approval, Workspace};
use kg_domain::{NotifyLevel, PluginId};
use kg_plugins::PluginHost;

use super::question;
use super::rules::RulesEditor;
use super::surface::mount_error;
use super::variables::Variables;
use super::{Kagan, OpenSettings};
use crate::theme::ThemeState;
use crate::ui_observe::UiObserveExt as _;

pub(super) use keyboard::{Recorder, plugin_commands};
use nav::Category;
pub(super) use plugins::Drafts;

actions!(
    settings,
    [
        CloseSettings,
        ActivateItem,
        NextItem,
        PreviousItem,
        OpenSection
    ]
);

pub(super) const CONTEXT: &str = "KaganSettings";

pub(super) const ITEM: &str = "SettingsItem";
const CONTEXT_SECTIONS: &str = "SettingsSections";

const CLOSE: &str = "SettingsClose";

pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", CloseSettings, Some(CONTEXT)),
        KeyBinding::new("enter", CloseSettings, Some(CLOSE)),
        KeyBinding::new("space", CloseSettings, Some(CLOSE)),
        KeyBinding::new("enter", ActivateItem, Some(ITEM)),
        KeyBinding::new("space", ActivateItem, Some(ITEM)),
        KeyBinding::new("down", NextItem, Some(ITEM)),
        KeyBinding::new("up", PreviousItem, Some(ITEM)),
        KeyBinding::new("enter", OpenSection, Some(CONTEXT_SECTIONS)),
    ]
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is an independent settings row or a read marker"
)]
pub(super) struct Prefs {
    pub(super) agent_integration: bool,
    pub(super) confirm_archive: bool,
    pub(super) notify: NotifyLevel,
    pub(super) plugin_status: bool,
    pub(super) update_check: bool,
    pub(super) loaded: bool,
}

pub(super) struct SettingsState {
    pub(super) scroll: ScrollHandle,
    pub(super) approvals: usize,
    pub(super) drafts: Drafts,
    pub(super) keyboard: Recorder,
    section: Category,
    pub(super) cursor: usize,
    pub(super) query: Entity<InputState>,
    _query_watch: Subscription,
    pub(super) list: FocusHandle,
    pub(super) variables: Entity<Variables>,
    pub(super) rules: Entity<RulesEditor>,
}

impl SettingsState {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Kagan>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings"));
        // A new query starts at the first result; an empty one puts the cursor back on the section.
        let watch = cx.subscribe(&query, |this, query, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                this.settings.cursor = if query.read(cx).value().trim().is_empty() {
                    nav::section_position(&this.settings.section, cx)
                } else {
                    0
                };
                cx.notify();
            }
        });
        let variables = cx.new(|cx| Variables::new(None, window, cx));
        let rules = cx.new(|cx| RulesEditor::new(None, window, cx));
        Self {
            variables,
            scroll: ScrollHandle::new(),
            approvals: 0,
            drafts: Drafts::default(),
            keyboard: Recorder::default(),
            section: Category::default(),
            cursor: 0,
            query,
            _query_watch: watch,
            list: cx.focus_handle().tab_stop(true),
            rules,
        }
    }
}

pub(super) fn state(cx: &mut App) -> Entity<Prefs> {
    cx.new(|_| Prefs {
        agent_integration: true,
        confirm_archive: false,
        notify: NotifyLevel::default(),
        plugin_status: true,
        update_check: false,
        loaded: false,
    })
}

pub(super) fn watch(prefs: &Entity<Prefs>, cx: &mut Context<Kagan>) -> [Subscription; 3] {
    [
        cx.observe(&kg_core::Updater::global(cx), |_, _, cx| cx.notify()),
        cx.observe(prefs, |_, _, cx| cx.notify()),
        cx.observe(&ThemeState::global(cx), |_, _, cx| cx.notify()),
    ]
}

impl Kagan {
    pub(super) fn toggle_settings(
        &mut self,
        _: &OpenSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.in_settings() {
            self.leave_settings(window, cx);
        } else {
            self.open_settings(window, cx);
        }
    }

    pub(crate) fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.in_settings() {
            self.screen.remember_center(window, cx);
        }
        self.open_center(super::screen::Center::Settings, window, cx);
        self.dismiss_thread_menus(cx);
        window.focus(&self.focus, cx);
        self.sync_panels_keeping_widths(window, cx);
        plugins::sync(&mut self.settings.drafts, window, cx);
        let rules = self.settings.rules.clone();
        rules.update(cx, |editor, cx| editor.refresh(window, cx));
        for (id, _) in pages::plugin_entries(cx) {
            self.mount_settings_view(&id, window, cx);
        }
    }

    pub(super) fn manage_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(window, cx);
        self.select_section(Category::Installed, window, cx);
    }

    pub(super) fn close_settings(
        &mut self,
        _: &CloseSettings,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.leave_settings(window, cx);
    }

    pub(super) fn leave_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.in_settings() {
            return;
        }
        self.stop_recording();
        let work = self.screen.shown().clone();
        self.open_center(super::screen::Center::Work(work), window, cx);
        self.announce_questions(window, cx);
        self.return_focus(super::screen::FocusReturn::Center, window, cx);
        cx.notify();
    }

    pub(super) fn reconcile_plugin_views(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        plugins::sync(&mut self.settings.drafts, window, cx);
        let entries = pages::plugin_entries(cx);
        let host = PluginHost::try_global(cx);
        self.slots.settings_views.retain(|id, (generation, _)| {
            entries.iter().any(|(entry, _)| entry == id)
                && host
                    .as_ref()
                    .is_some_and(|host| host.read(cx).generation(id) == Some(*generation))
        });
        if self.in_settings() {
            for (id, _) in entries {
                self.mount_settings_view(&id, window, cx);
            }
        }
        cx.notify();
    }

    fn mount_settings_view(&mut self, id: &PluginId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = PluginHost::try_global(cx) else {
            return;
        };
        let export = kg_plugins::contributions(cx).ok().and_then(|list| {
            list.into_iter()
                .find(|(plugin, _)| plugin == id)
                .and_then(|(_, ui)| ui.settings.as_deref().map(str::to_owned))
        });
        let Some(export) = export else {
            self.slots.settings_views.remove(id);
            return;
        };
        let generation = host.read(cx).generation(id);
        let mounted = self
            .slots
            .settings_views
            .get(id)
            .is_some_and(|(have, _)| Some(*have) == generation);
        if mounted && generation.is_some() {
            return;
        }
        match host.update(cx, |host, cx| {
            host.mount_named_view(id, &export, window, cx)
        }) {
            Ok(view) => {
                let generation = host.read(cx).generation(id).unwrap_or_default();
                self.slots
                    .settings_views
                    .insert(id.clone(), (generation, view));
            }
            Err(error) => {
                self.note = Some(
                    format!(
                        "Could not open the settings of {}: {}",
                        id.0,
                        mount_error(&error)
                    )
                    .into(),
                );
            }
        }
    }

    pub(super) fn load_settings(&mut self, cx: &mut Context<Self>) {
        self.settings
            .variables
            .update(cx, super::variables::Variables::reload);
        Self::load_layout(cx);
        self.load_prefs(cx);
        self.load_slots(cx);
        self.load_terminal_size(cx);
    }

    pub(super) fn load_prefs(&self, cx: &mut Context<Self>) {
        let agent = super::layout::read::<bool>(kg_domain::prefs::AGENT_INTEGRATION, cx);
        let notify = super::layout::read::<NotifyLevel>(kg_domain::prefs::NOTIFY, cx);
        let plugin_status = super::layout::read::<bool>(kg_domain::prefs::PLUGIN_STATUS, cx);
        let update = super::layout::read::<bool>(kg_domain::prefs::UPDATE_CHECK, cx);
        let confirm = super::layout::read::<bool>(kg_domain::prefs::CONFIRM_ARCHIVE, cx);
        let prefs = self.prefs.clone();
        cx.spawn(async move |_, cx| {
            let on = agent.await.ok().flatten().unwrap_or(true);
            let notifications = notify.await.ok().flatten().unwrap_or_default();
            let status = plugin_status.await.ok().flatten().unwrap_or(true);
            let update_check = update.await.ok().flatten().unwrap_or(false);
            let confirm_archive = confirm.await.ok().flatten().unwrap_or(false);
            prefs.update(cx, |prefs, cx| {
                prefs.confirm_archive = confirm_archive;
                prefs.agent_integration = on;
                prefs.notify = notifications;
                prefs.plugin_status = status;
                prefs.update_check = update_check;
                prefs.loaded = true;
                cx.notify();
            });
        })
        .detach();
    }
}

pub(super) fn main(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let groups = pages::groups(kagan, window, cx);
    let entries = nav::entries(kagan, &groups, cx);
    let sidebar = nav::sidebar(kagan, &entries, window, cx);
    let approvals: Vec<Approval> = Workspace::global(cx)
        .read(cx)
        .approvals()
        .cloned()
        .collect();
    let entity = cx.entity();
    let weak = entity.downgrade();
    let cards: Vec<AnyElement> = approvals
        .iter()
        .map(|approval| {
            question::card(
                question::Question::standalone(approval),
                None,
                &entity,
                &weak,
                window,
                cx,
            )
            .into_any_element()
        })
        .collect();
    // Questions is the first child only while a question waits; a new approval scrolls to the top.
    let mut items = Vec::new();
    if !cards.is_empty() {
        items.push(
            v_flex()
                .gap(px(space(SpaceStep::Sm, cx)))
                .child("Questions")
                .children(cards)
                .into_any_element(),
        );
    }
    let section = nav::shown(&kagan.settings.section, cx);
    items.extend(
        groups
            .into_iter()
            .filter(|group| group.section == section)
            .map(|group| group_block(group, cx)),
    );
    items.extend(plugin_view(kagan, &section));
    let page = v_flex()
        .id("settings-page")
        .size_full()
        .items_center()
        .p(px(space(SpaceStep::Xl, cx)))
        .gap(px(space(SpaceStep::Xl, cx)))
        .track_scroll(&kagan.settings.scroll)
        .overflow_y_scroll()
        .children(
            items
                .into_iter()
                .map(|item| div().w_full().max_w(rems(39.5)).child(item)),
        )
        .ui_container();
    let content = div()
        .relative()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(page)
        .vertical_scrollbar(&kagan.settings.scroll);
    v_flex()
        .size_full()
        .child(
            div()
                .px(px(space(SpaceStep::Xl, cx)))
                .pt(px(space(SpaceStep::Xl, cx)))
                .child(settings_header(window, cx)),
        )
        .child(
            h_flex()
                .flex_1()
                .min_h_0()
                .w_full()
                .child(sidebar)
                .child(content),
        )
        .into_any_element()
}

fn plugin_view(kagan: &Kagan, section: &Category) -> Option<AnyElement> {
    let Category::Plugin(id) = section else {
        return None;
    };
    let (_, view) = kagan.slots.settings_views.get(id)?;
    Some(
        div()
            .id("settings-plugin-view")
            .ui_container()
            .h(rems(20.))
            .child(view.clone())
            .into_any_element(),
    )
}

fn settings_header(window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let close = super::tab_stop(window, cx, "settings-close-focus");
    let radius = px(radius_md(cx));
    h_flex()
        .justify_between()
        .items_center()
        .child(
            div()
                .id("settings-title")
                .ui_container()
                .type_title(cx)
                .font_semibold()
                .child("Settings"),
        )
        .child(
            div()
                .id("settings-close")
                .key_context(CLOSE)
                .track_focus(&close)
                .px(px(space(SpaceStep::Sm, cx)))
                .py(px(space(SpaceStep::Xs, cx)))
                .rounded(radius)
                .type_body(cx)
                .focus(|close| close.border_1().border_color(cx.theme().ring))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.leave_settings(window, cx);
                }))
                .child("Close")
                .ui_observe_as(Facts {
                    focus: Some(close.clone()),
                    ..Facts::button("Close")
                }),
        )
        .into_any_element()
}

fn group_block(group: row::Group, cx: &App) -> AnyElement {
    let radius = cx.theme().radius;
    let line = cx.theme().border;
    let card = cx.theme().group_box;
    let muted = cx.theme().muted_foreground;
    let rows = group.rows.len();
    let shortcut = group.style != row::GroupStyle::Plain;
    let first_shortcut = group.style == row::GroupStyle::FirstShortcuts;
    let title = group.title.clone();
    let block =
        v_flex()
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
                    .children(group.rows.into_iter().enumerate().map(|(ix, row)| {
                        row_element(row, ix + 1 == rows, line, muted, shortcut, cx)
                    })),
            )
            .into_any_element();
    if first_shortcut {
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

fn row_element(
    row: row::Row,
    last: bool,
    line: Hsla,
    muted: Hsla,
    shortcut: bool,
    cx: &App,
) -> AnyElement {
    let show_text = !row.bare;
    let noted = !row.note.is_empty();
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
    let item = div()
        .id(row.id)
        .ui_container()
        .aria_label(row.title)
        .when(shortcut, |item| item.px(px(space(SpaceStep::Lg, cx))))
        .when(shortcut && !noted, |item| item.h(px(control_height(cx))))
        .when(shortcut && noted, |item| {
            item.py(px(space(SpaceStep::Xs, cx)))
        })
        .when(!shortcut, |item| {
            item.px(px(space(SpaceStep::Lg, cx)))
                .py(px(space(SpaceStep::Md, cx)))
        })
        .when(!last, |item| item.border_b_1().border_color(line))
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

#[cfg(test)]
pub(crate) fn text_for_test(plugin: &PluginId, key: &str, cx: &App) -> String {
    plugins::text_for_test(plugin, key, cx)
}
