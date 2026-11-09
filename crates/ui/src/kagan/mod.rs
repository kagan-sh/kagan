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
pub(crate) mod menus;
mod navigation;
mod needs;
mod notify;
mod overlays;
pub(crate) mod picker;
mod plugin_actions;
mod preview;
mod question;
pub(crate) mod quit;
mod replace;
mod review;
mod right;
mod screen;
mod settings;
mod shown;
mod sidebar;
mod start;
mod surface;
mod tabs;
mod update;
mod web_panel;
pub(crate) use web_panel::drop_plugin as drop_web_panel;

mod events;
mod files;
mod font;
mod host;
mod render;
mod startup;
#[cfg(test)]
mod tests;
mod threads;
#[cfg(test)]
pub(crate) use tests::*;
mod toast;
use toast::Toast;

pub(crate) use keys::bind;
pub(crate) use keys::install_keys;
pub(crate) use surface::Commit as SurfaceCommit;
pub(crate) use surface::paint_then_settle;
mod window;

pub(crate) use window::shows;
pub(crate) use window::{
    front, show, show_plugin_settings, show_preview, show_settings, start_plugin, watch_waiting,
};

use std::collections::BTreeSet;

use crate::terminal::TerminalView;
use gpui_kit::component::input::InputState;
use gpui_kit::component::resizable::ResizableState;
use gpui_kit::{
    AnyWindowHandle, App, ElementId, Entity, FocusHandle, SharedString, Subscription, Task, Window,
    actions,
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
        OpenThemes,
        Preview
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
    slots: surface::SlotState,
    typing: bool,
    quiet: Task<()>,
    on_screen: BTreeSet<PendingInteractionId>,
    announced: BTreeSet<PendingInteractionId>,
    typed_watch: Option<Subscription>,
    changes: Changes,
    details: details::Details,
    tabs: tabs::Tabs,
    review_drafts: review::Drafts,
    plugins: Plugins,
    right: right::Sidebar,
    layout: Layout,
    viewport: gpui_kit::Pixels,
    prefs: Entity<settings::Prefs>,
    _settings_watch: [Subscription; 3],
    panes: layout::Panes,
    left_overlay_focus: FocusHandle,
    start_on_load: Option<ProjectId>,
    settings: settings::SettingsState,
    settling: bool,
    columns: Entity<ResizableState>,
    window: AnyWindowHandle,
    title: shown::TitleState,
    open_thread: Option<Thread>,
    retired_load: Task<()>,
    archive_query: Entity<InputState>,
    _archive_watch: Subscription,
    browser: browser::Browser,
    _subscriptions: [Subscription; 5],
    _sidebar_watch: [Subscription; 2],
    _appearance: Subscription,
    _theme_paint: Subscription,
    _theme_notice: Subscription,
    asks: ask_form::AskState,
    toast: Option<Toast>,
    toast_epoch: u64,
    activity: activity::Strip,
    _update_watch: Subscription,
    _release: Subscription,
    _poll: Task<()>,
    commands: Option<commands::CommandList>,
}
