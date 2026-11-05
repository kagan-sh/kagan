//! The scrolling settings page. Esc or Close returns to the window.

use crate::theme::{SpaceStep, TypeScale as _, control_height, radius_md, space};

mod indexes;
mod installed;
mod keyboard;
mod pages;
mod plugins;
mod row;
mod slots;
mod updates;

use crate::ui_registry::Facts;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, Hsla, InteractiveElement as _,
    IntoElement, KeyBinding, ParentElement as _, StatefulInteractiveElement as _, Styled as _,
    Subscription, Window, actions, div, px, rems,
};
use kg_core::{Approval, Workspace};
use kg_domain::PluginId;
use kg_plugins::PluginHost;

use super::question;
use super::surface::mount_error;
use super::{Kagan, OpenSettings};
use crate::theme::ThemeState;
use crate::ui_observe::UiObserveExt as _;

pub(super) use keyboard::{Recorder, plugin_commands};
pub(super) use plugins::Drafts;

actions!(
    settings,
    [CloseSettings, ActivateItem, NextItem, PreviousItem]
);

pub(super) const CONTEXT: &str = "KaganSettings";

/// The context of a control the keyboard can reach: a menu entry, a theme card.
pub(super) const ITEM: &str = "SettingsItem";

const CLOSE: &str = "SettingsClose";

/// Esc goes back. Enter and Space activate the focused control, and the arrows move focus.
pub(crate) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", CloseSettings, Some(CONTEXT)),
        KeyBinding::new("enter", CloseSettings, Some(CLOSE)),
        KeyBinding::new("space", CloseSettings, Some(CLOSE)),
        KeyBinding::new("enter", ActivateItem, Some(ITEM)),
        KeyBinding::new("space", ActivateItem, Some(ITEM)),
        KeyBinding::new("down", NextItem, Some(ITEM)),
        KeyBinding::new("up", PreviousItem, Some(ITEM)),
    ]
}

#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is one independent row on the settings page"
)]
pub(super) struct Prefs {
    pub(super) agent_integration: bool,
    pub(super) notify: bool,
    pub(super) plugin_status: bool,
    pub(super) update_check: bool,
}

pub(super) fn state(cx: &mut App) -> Entity<Prefs> {
    cx.new(|_| Prefs {
        agent_integration: true,
        notify: true,
        plugin_status: true,
        update_check: false,
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
        plugins::sync(&mut self.settings_drafts, window, cx);
        for (id, _) in pages::plugin_entries(cx) {
            self.mount_settings_view(&id, window, cx);
        }
    }

    /// Opens Settings on the Installed page, where plugins are managed.
    pub(super) fn manage_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(window, cx);
        let kagan = cx.entity();
        window.defer(cx, move |_, cx| {
            kagan.update(cx, |this, cx| {
                this.settings_scroll
                    .scroll_to_top_of_item(5 + keyboard::count(cx));
                cx.notify();
            });
        });
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
        plugins::sync(&mut self.settings_drafts, window, cx);
        let entries = pages::plugin_entries(cx);
        let host = PluginHost::try_global(cx);
        self.settings_views.retain(|id, (generation, _)| {
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

    /// Mounts a plugin's own settings view, when it declares one. A view is mounted again when
    /// its plugin has loaded since, and dropped when the plugin declares none.
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
            self.settings_views.remove(id);
            return;
        };
        let generation = host.read(cx).generation(id);
        let mounted = self
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
                self.settings_views.insert(id.clone(), (generation, view));
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

    /// Reads the sidebar layout and the preferences the window shows, at start and on each change.
    pub(super) fn load_settings(&mut self, cx: &mut Context<Self>) {
        Self::load_layout(cx);
        self.load_prefs(cx);
        self.load_slots(cx);
        self.load_terminal_size(cx);
    }

    /// Reads the preferences the page shows, so a change from `kg` shows at once.
    pub(super) fn load_prefs(&self, cx: &mut Context<Self>) {
        let agent = super::layout::read::<bool>("agent-integration", cx);
        let notify = super::layout::read::<bool>("notify", cx);
        let plugin_status = super::layout::read::<bool>("plugin-status", cx);
        let update = super::layout::read::<bool>("update-check", cx);
        let prefs = self.prefs.clone();
        cx.spawn(async move |_, cx| {
            let on = agent.await.ok().flatten().unwrap_or(true);
            let notifications = notify.await.ok().flatten().unwrap_or(true);
            let status = plugin_status.await.ok().flatten().unwrap_or(true);
            let update_check = update.await.ok().flatten().unwrap_or(false);
            prefs.update(cx, |prefs, cx| {
                prefs.agent_integration = on;
                prefs.notify = notifications;
                prefs.plugin_status = status;
                prefs.update_check = update_check;
                cx.notify();
            });
        })
        .detach();
    }
}

pub(super) fn main(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let groups = pages::groups(kagan, window, cx);
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
    let views: Vec<_> = pages::plugin_entries(cx)
        .into_iter()
        .filter_map(|(id, name)| {
            kagan
                .settings_views
                .get(&id)
                .map(|(_, view)| (id, name, view))
        })
        .enumerate()
        .map(|(index, (id, name, view))| {
            v_flex()
                .gap(px(space(SpaceStep::Sm, cx)))
                .child(div().type_caption(cx).font_semibold().child(name))
                .child(
                    div()
                        .id(ElementId::Name(
                            format!("settings-plugin-view-{}", id.0).into(),
                        ))
                        .ui_container()
                        .h(rems(20.))
                        .child(
                            div()
                                .id(ElementId::Name(if index == 0 {
                                    "settings-plugin-view".into()
                                } else {
                                    format!("settings-plugin-content-{}", id.0).into()
                                }))
                                .ui_container()
                                .size_full()
                                .child(view.clone()),
                        ),
                )
        })
        .collect();
    let items = [
        settings_header(window, cx),
        v_flex()
            .gap(px(space(SpaceStep::Sm, cx)))
            .when(!cards.is_empty(), |block| {
                block.child("Questions").children(cards)
            })
            .into_any_element(),
    ]
    .into_iter()
    .chain(groups.into_iter().map(|group| group_block(group, cx)))
    .chain(views.into_iter().map(IntoElement::into_any_element));
    // Items stay direct children of the scroller: `scroll_to_top_of_item` counts them.
    // The bar sits on a still parent; inside the scroller it would scroll away with the rows.
    div()
        .relative()
        .size_full()
        .child(
            v_flex()
                .id("settings-page")
                .size_full()
                .items_center()
                .p(px(space(SpaceStep::Xl, cx)))
                .gap(px(space(SpaceStep::Xl, cx)))
                .track_scroll(&kagan.settings_scroll)
                .overflow_y_scroll()
                .children(items.map(|item| div().w_full().max_w(rems(39.5)).child(item)))
                .ui_container(),
        )
        .vertical_scrollbar(&kagan.settings_scroll)
        .into_any_element()
}

fn settings_header(window: &mut Window, cx: &mut Context<Kagan>) -> AnyElement {
    let close = super::tab_stop(window, cx, "settings-close-focus");
    let radius = px(radius_md(cx));
    h_flex()
        .justify_between()
        .items_center()
        .child(div().type_title(cx).font_semibold().child("Settings"))
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
    let shortcut = group.key.starts_with("shortcuts-");
    let first_shortcut = group.key == "shortcuts-window";
    let title = group.title.clone();
    let block =
        v_flex()
            .flex_shrink_0()
            .id(ElementId::Name(
                format!("settings-group-{}", group.key).into(),
            ))
            .ui_container()
            .gap(px(space(SpaceStep::Sm, cx)))
            .when(first_shortcut, |card| {
                card.child(
                    div()
                        .type_caption(cx)
                        .font_semibold()
                        .text_color(muted)
                        .child("Keyboard"),
                )
            })
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
                        .text_color(muted)
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

/// One row of a card. A wide control, such as the theme cards, sits under its title instead of
/// beside it, so the title keeps its width.
fn row_element(
    row: row::Row,
    last: bool,
    line: Hsla,
    muted: Hsla,
    shortcut: bool,
    cx: &App,
) -> AnyElement {
    let show_text = row.id != "setting-row-install-form";
    let noted = !row.note.is_empty();
    let alias = if row.id == "setting-row-theme" {
        Some("theme")
    } else if row.id == "setting-row-mode" {
        Some("mode")
    } else {
        None
    };
    let note = row
        .note_content
        .unwrap_or_else(|| row.note.clone().into_any_element());
    let text = v_flex()
        .min_w_0()
        .flex_1()
        .gap_0p5()
        .when(show_text && !row.title.is_empty(), |text| {
            let title = div().type_body(cx).child(row.title.clone());
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
        .child(row.control);
    let item = div()
        .id(row.id)
        .ui_container()
        .aria_label(row.title)
        .when(shortcut, |item| item.px(px(space(SpaceStep::Lg, cx))))
        .when(shortcut && !noted, |item| item.h(px(control_height(cx))))
        .when(shortcut && noted, |item| {
            item.py(px(space(SpaceStep::Xs, cx)))
        })
        .when(!shortcut, |item| item.p(px(space(SpaceStep::Xl, cx))))
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
