use changes::Changes;
use layout::Layout;
use plugin_actions::Plugins;
mod activity;
mod archived;
mod ask_form;
pub(crate) mod attention;
pub(crate) mod browser;
pub(crate) mod changes;
pub(crate) mod commands;
mod controls;
mod details;
#[cfg(target_os = "macos")]
mod dock;
pub(crate) mod header;
mod keys;
mod layout;
#[cfg(test)]
pub(crate) use layout::read as layout_read;
pub(crate) mod menus;
mod navigation;
mod needs;
mod notify;
mod overlays;
pub(crate) mod picker;
mod plugin_actions;
mod question;
pub(crate) mod quit;
mod replace;
#[cfg(test)]
pub(crate) use replace::read as replace_read;
mod review;
#[cfg(test)]
pub(crate) use review::read as review_read;
mod right;
mod screen;
#[cfg(test)]
pub(crate) mod screen_read;
mod settings;
mod shown;
mod sidebar;
mod start;
mod surface;
mod tabs;
mod update;
mod web_panel;
pub(crate) use web_panel::drop_plugin as drop_web_panel;
#[cfg(all(test, target_os = "macos"))]
pub(crate) use web_panel::{forget, inject, mark_stale, post, queue_page, testing_page};

mod events;
mod files;
mod font;
mod host;
mod render;
mod startup;
#[cfg(test)]
mod test_support;
mod threads;
mod toast;
use toast::Toast;

pub(crate) use keys::bind;
#[cfg(test)]
pub(crate) use keys::chord_for;
pub(crate) use keys::install_keys;
#[cfg(test)]
pub(crate) use keys::is_chord;
#[cfg(test)]
pub(crate) use needs::QUIET;
pub(crate) use surface::Commit as SurfaceCommit;
pub(crate) use surface::paint_then_settle;
#[cfg(test)]
pub(crate) use tabs::{CloseTerminal, NewTerminal, NextTerminal, OpenLocalLink, PreviousTerminal};
mod window;

#[cfg(test)]
pub(crate) use threads::description;
#[cfg(test)]
pub(crate) use window::open;
pub(crate) use window::shows;
#[cfg(test)]
pub(crate) use window::window_options;
pub(crate) use window::{
    front, show, show_plugin_settings, show_settings, start_plugin, watch_waiting,
};

use std::collections::{BTreeMap, BTreeSet};

use crate::terminal::TerminalView;
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::resizable::ResizableState;
use gpui_kit::{
    AnyWindowHandle, App, ElementId, Entity, FocusHandle, ScrollHandle, SharedString, Subscription,
    Task, Window, actions,
};
use kg_domain::{PendingInteractionId, ProjectId, Thread, ThreadId};

pub(crate) const TITLE: &str = "Kagan";
const MIN_HEIGHT: f32 = 360.;
pub(crate) const CONTEXT: &str = kg_plugins::keys::WINDOW_CONTEXT;

fn tab_stop(window: &mut Window, cx: &mut App, key: impl Into<ElementId>) -> FocusHandle {
    window
        .use_keyed_state(key, cx, |_, cx| cx.focus_handle().tab_stop(true))
        .read(cx)
        .clone()
}

actions!(
    kagan,
    [
        NewThread,
        StopThread,
        ArchiveThread,
        TogglePinned,
        StartAgain,
        SaveDiagnostics,
        AddProject,
        FocusSidebar,
        RestoreThread,
        ToggleSidebar,
        ToggleRightSidebar,
        DismissRightOverlay,
        DismissLeftOverlay,
        OpenSettings,
        UseProjectFolder,
        UseWorktreeFolder,
        RetryBranches,
        RetryPrograms,
        ToggleCustomCommand,
        CloseCustomCommand,
        ThreadActions,
        RenameThread,
        Bigger,
        Smaller,
        ActualSize,
        NewTheme,
        OpenThemes
    ]
);

#[derive(Clone)]
enum Shown {
    Nothing,
    Start(ProjectId),
    Thread(Entity<TerminalView>),
}

pub(crate) struct Kagan {
    screen: screen::Screen,
    terminal_font_size: u8,
    terminal_font_epoch: u64,
    window_title: String,
    sidebar: Entity<sidebar::Sidebar>,
    picker: Entity<picker::Picker>,
    start: start::StartState,
    questions: Entity<attention::Attention>,
    _questions_watch: Subscription,
    note: Option<SharedString>,
    plugin_prompt_notice: Option<ThreadId>,
    focus: FocusHandle,
    surface_close_focus: FocusHandle,
    // While a surface open or close waits for its frame, sidebar and collapse preferences stay as they were.
    surface_hold: bool,
    typing: bool,
    quiet: Task<()>,
    /// The questions that were on screen when the person started typing. They stay up.
    on_screen: BTreeSet<PendingInteractionId>,
    /// The person-only questions already seen, so a new one opens the panels once.
    announced: BTreeSet<PendingInteractionId>,
    typed_watch: Option<Subscription>,
    changes: Changes,
    details: details::Details,
    tabs: tabs::Tabs,
    review_drafts: review::Drafts,
    replaced: replace::Replaced,
    plugins: Plugins,
    panels: Vec<right::Panel>,
    right_slots: Vec<surface::Slot>,
    right_tab: Option<String>,
    right_menu: Option<(Entity<PopupMenu>, Subscription)>,
    right_add_focus: FocusHandle,
    right_fill: right::Fill,
    right_panels: Vec<surface::Declared>,
    tool: Option<surface::Slot>,
    surface_check: Option<surface::Check>,
    surface_epoch: u64,
    layout: Layout,
    viewport: gpui_kit::Pixels,
    prefs: Entity<settings::Prefs>,
    _settings_watch: [Subscription; 3],
    panes: layout::Panes,
    right_toggle_focus: FocusHandle,
    left_overlay_focus: FocusHandle,
    /// Enter on a project is waiting for the picker to load its program.
    start_on_load: Option<ProjectId>,
    settings_scroll: ScrollHandle,
    settings_approvals: usize,
    settings_drafts: settings::Drafts,
    keyboard: settings::Recorder,
    /// Set while a sidebar change made on leaving Settings is still being delivered
    settling: bool,
    /// A plugin's own settings view, with the plugin's load generation it was mounted from
    settings_views: BTreeMap<kg_domain::PluginId, (u64, Entity<gpui_shell::ScriptView>)>,
    columns: Entity<ResizableState>,
    window: AnyWindowHandle,
    actions_menu: Option<(Entity<PopupMenu>, Subscription)>,
    open_thread: Option<Thread>,
    retired_load: Task<()>,
    archive_query: Entity<InputState>,
    _archive_watch: Subscription,
    browser: browser::Browser,
    title_input: Option<Entity<InputState>>,
    title_watch: Option<Subscription>,
    _subscriptions: [Subscription; 5],
    _sidebar_watch: [Subscription; 2],
    _appearance: Subscription,
    _theme_paint: Subscription,
    _theme_notice: Subscription,
    plugin_watch: Option<Subscription>,
    host_events: Vec<Subscription>,
    overlays: Vec<overlays::Overlay>,
    ask_drafts: BTreeMap<PendingInteractionId, ask_form::Draft>,
    ask_hidden: BTreeSet<PendingInteractionId>,
    ask_popup: Option<FocusHandle>,
    toast: Option<Toast>,
    toast_epoch: u64,
    activity: activity::Strip,
    _update_watch: Subscription,
    _release: Subscription,
    _poll: Task<()>,
    commands: Option<commands::CommandList>,
}
