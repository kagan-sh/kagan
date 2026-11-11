use crate::theme::{SpaceStep, TypeScale as _, radius_md, space};

mod appearance;
mod command;
mod fonts;
mod general;
mod indexes;
mod installed;
mod render;
pub(in crate::kagan) use installed::update_outcome;
mod keyboard;
mod nav;
mod pages;
mod plugins;
pub(super) mod project;
mod row;
mod slots;
mod suggested;
mod updates;

use crate::ui_registry::Facts;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::searchable_list::SearchableVec;
use gpui_kit::component::select::{SelectEvent, SelectState};
use gpui_kit::component::{ActiveTheme as _, StyledExt as _};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, InteractiveElement as _,
    IntoElement, KeyBinding, ParentElement as _, ScrollHandle, StatefulInteractiveElement as _,
    Styled as _, Subscription, Window, actions, div, px, rems,
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

use fonts::{FontItem, FontSelect, font_items};
pub(super) use keyboard::{Recorder, plugin_commands};
use nav::Category;
pub(super) use plugins::Drafts;
use render::group_block;

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
    pub(super) copy_on_select: bool,
    pub(super) links_in_tab: bool,
    pub(super) setup_guide: Option<bool>,
    pub(super) agent_ready: Option<bool>,
    pub(super) kg_check: Option<command::Check>,
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
    pub(super) branch_prefix: Entity<InputState>,
    _branch_watch: Subscription,
    pub(super) ui_font: Entity<FontSelect>,
    pub(super) terminal_font: Entity<FontSelect>,
    _font_watch: [Subscription; 2],
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
        let branch_prefix = cx.new(|cx| InputState::new(window, cx).placeholder("bb/"));
        // The prefix is saved on Enter or when the field loses focus; a bad one stays unsaved.
        let branch_watch = cx.subscribe_in(
            &branch_prefix,
            window,
            |_, input, event: &InputEvent, _, cx| {
                if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                    let text = input.read(cx).value().trim().to_owned();
                    if kg_domain::prefs::valid_branch_prefix(&text) {
                        super::layout::save(kg_domain::prefs::BRANCH_PREFIX, text, cx);
                    }
                }
            },
        );
        let variables = cx.new(|cx| Variables::new(None, window, cx));
        let rules = cx.new(|cx| RulesEditor::new(None, window, cx));
        let fonts = font_items(cx);
        let ui_font = cx.new(|cx| {
            SelectState::new(SearchableVec::new(fonts.clone()), None, window, cx).searchable(true)
        });
        let terminal_font = cx.new(|cx| {
            SelectState::new(SearchableVec::new(fonts), None, window, cx).searchable(true)
        });
        let ui_watch = cx.subscribe_in(
            &ui_font,
            window,
            |_, _, event: &SelectEvent<SearchableVec<FontItem>>, _, cx| {
                if let SelectEvent::Confirm(Some(name)) = event {
                    super::layout::save(kg_domain::prefs::FONT_UI, name.clone(), cx);
                }
            },
        );
        let terminal_watch = cx.subscribe_in(
            &terminal_font,
            window,
            |_, _, event: &SelectEvent<SearchableVec<FontItem>>, _, cx| {
                if let SelectEvent::Confirm(Some(name)) = event {
                    super::layout::save(kg_domain::prefs::FONT_TERMINAL, name.clone(), cx);
                }
            },
        );
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
            branch_prefix,
            _branch_watch: branch_watch,
            ui_font,
            terminal_font,
            _font_watch: [ui_watch, terminal_watch],
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
        copy_on_select: false,
        links_in_tab: false,
        setup_guide: None,
        agent_ready: None,
        kg_check: None,
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
        ThemeState::global(cx).update(cx, ThemeState::end_preview);
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
        self.load_kg_on_path(cx);
        self.load_slots(cx);
        self.load_terminal_size(cx);
    }

    // Looks for kg on the login PATH and beside kagan. Runs off the main thread, and gives up after a few seconds.
    pub(super) fn load_kg_on_path(&self, cx: &mut Context<Self>) {
        let prefs = self.prefs.clone();
        let appimage = std::env::var_os("APPIMAGE").is_some();
        cx.spawn(async move |_, cx| {
            let check = cx
                .background_executor()
                .spawn(async move { command::probe(appimage) })
                .await;
            prefs.update(cx, |prefs, cx| {
                prefs.kg_check = Some(check);
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn load_prefs(&self, cx: &mut Context<Self>) {
        let agent = super::layout::read::<bool>(kg_domain::prefs::AGENT_INTEGRATION, cx);
        let notify = super::layout::read::<NotifyLevel>(kg_domain::prefs::NOTIFY, cx);
        let plugin_status = super::layout::read::<bool>(kg_domain::prefs::PLUGIN_STATUS, cx);
        let update = super::layout::read::<bool>(kg_domain::prefs::UPDATE_CHECK, cx);
        let confirm = super::layout::read::<bool>(kg_domain::prefs::CONFIRM_ARCHIVE, cx);
        let branch = super::layout::read::<String>(kg_domain::prefs::BRANCH_PREFIX, cx);
        let ui_name = super::layout::read::<String>(kg_domain::prefs::FONT_UI, cx);
        let terminal_name = super::layout::read::<String>(kg_domain::prefs::FONT_TERMINAL, cx);
        let ui_font = self.settings.ui_font.clone();
        let terminal_font = self.settings.terminal_font.clone();
        let copy = super::layout::read::<bool>(kg_domain::prefs::TERMINAL_COPY_ON_SELECT, cx);
        let links = super::layout::read::<bool>(kg_domain::prefs::TERMINAL_LINKS_BROWSER, cx);
        let branch_input = self.settings.branch_prefix.clone();
        let fonts = font_items(cx);
        let window = self.window;
        let setup = super::layout::read::<bool>(kg_domain::prefs::SETUP_GUIDE, cx);
        let list_order = super::layout::read::<String>(kg_domain::prefs::SIDEBAR_ORDER, cx);
        let hide_empty =
            super::layout::read::<bool>(kg_domain::prefs::SIDEBAR_HIDE_EMPTY_PROJECTS, cx);
        let unread = super::layout::read::<bool>(kg_domain::prefs::SIDEBAR_UNREAD_FIRST, cx);
        let sidebar = self.sidebar.clone();
        let kagan = cx.weak_entity();
        let prefs = self.prefs.clone();
        cx.spawn(async move |_, cx| {
            let list_order = list_order.await.ok().flatten();
            let hide_empty_projects = hide_empty.await.ok().flatten().unwrap_or(false);
            let unread_first = unread.await.ok().flatten().unwrap_or(false);
            sidebar.update(cx, |sidebar, cx| {
                sidebar.apply_options(list_order.as_deref(), hide_empty_projects, unread_first, cx);
            });
            let branch_prefix = branch.await.ok().flatten().unwrap_or_default();
            let ui_name = ui_name.await.ok().flatten().unwrap_or_default();
            let terminal_name = terminal_name.await.ok().flatten().unwrap_or_default();
            _ = cx.update_window(window, |_, window, cx| {
                branch_input.update(cx, |state, cx| state.set_value(branch_prefix, window, cx));
                ui_font.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(fonts.clone()), window, cx);
                    state.set_selected_value(&ui_name, window, cx);
                });
                terminal_font.update(cx, |state, cx| {
                    state.set_items(SearchableVec::new(fonts), window, cx);
                    state.set_selected_value(&terminal_name, window, cx);
                });
            });
            let on = agent.await.ok().flatten().unwrap_or(true);
            let notifications = notify.await.ok().flatten().unwrap_or_default();
            let status = plugin_status.await.ok().flatten().unwrap_or(true);
            let update_check = update.await.ok().flatten().unwrap_or(false);
            let confirm_archive = confirm.await.ok().flatten().unwrap_or(false);
            let copy_on_select = copy.await.ok().flatten().unwrap_or(false);
            let links_in_tab = links.await.ok().flatten().unwrap_or(false);
            let setup_guide = setup.await.ok().flatten();
            prefs.update(cx, |prefs, cx| {
                prefs.confirm_archive = confirm_archive;
                prefs.agent_integration = on;
                prefs.notify = notifications;
                prefs.plugin_status = status;
                prefs.update_check = update_check;
                prefs.copy_on_select = copy_on_select;
                prefs.links_in_tab = links_in_tab;
                prefs.setup_guide = setup_guide;
                prefs.loaded = true;
                cx.notify();
            });
            _ = kagan.update(cx, Kagan::settle_setup_guide);
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

#[cfg(test)]
pub(crate) fn text_for_test(plugin: &PluginId, key: &str, cx: &App) -> String {
    plugins::text_for_test(plugin, key, cx)
}
