use crate::theme::TypeScale as _;
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

use changes::Changes;
use kg_domain::ProjectOp;
use layout::Layout;
use plugin_actions::Plugins;
use screen::{Activation, Side};
use sidebar::{Event, Shows};
use surface::ClosePage;

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

use std::collections::{BTreeMap, BTreeSet};

use crate::terminal::{TerminalView, Typed};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::InputState;
use gpui_kit::component::menu::PopupMenu;
use gpui_kit::component::resizable::{
    ResizablePanel, ResizablePanelEvent, ResizableState, h_resizable, resizable_panel,
};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::component::{TitleBar, WindowExt as _, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, AnyWindowHandle, App, AppContext as _, ClickEvent, Context, ElementId, Entity,
    FocusHandle, Global, InteractiveElement as _, IntoElement, ParentElement as _,
    PathPromptOptions, Render, ScrollHandle, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, Task, WeakEntity, Window, WindowBounds, WindowOptions, actions, div,
    px, size,
};
use kg_core::{Workspace, WorkspaceEvent};
use kg_domain::{
    Op, OpError, OpResult, PendingInteractionId, PluginId, PreferenceOp, ProjectId, SurfaceOp,
    ThemeOp, Thread, ThreadId, ThreadOp, ThreadStatus, archive_span,
};
use kg_plugins::{Follows, HostUi, HostUiEvent, PluginHost, PluginHostEvent, RunCommand};

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

#[derive(Clone)]
struct Toast {
    message: SharedString,
    action: Option<ToastCommand>,
    undo: Vec<ThreadId>,
}

impl Toast {
    fn said(message: String) -> Self {
        Self {
            message: message.into(),
            action: None,
            undo: Vec::new(),
        }
    }
}

#[derive(Clone)]
struct ToastCommand {
    plugin: kg_domain::PluginId,
    command: String,
    label: SharedString,
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
    _release: Subscription,
    _poll: Task<()>,
    commands: Option<commands::CommandList>,
}

struct OpenWindow {
    handle: AnyWindowHandle,
    kagan: WeakEntity<Kagan>,
}
impl Global for OpenWindow {}

pub(crate) fn open(cx: &mut App) -> gpui_kit::Result<Entity<Kagan>> {
    let (handle, kagan) = gpui_kit::open_window(window_options(cx), cx, |window, cx| {
        cx.new(|cx| Kagan::new(window, cx))
    })?;
    cx.set_global(OpenWindow {
        handle,
        kagan: kagan.downgrade(),
    });
    Ok(kagan)
}

/// Brings the app and its window to the front. A window that is already open comes forward; a
/// new one opens only when there is none.
pub(crate) fn show(cx: &mut App) -> gpui_kit::Result<()> {
    cx.activate(true);
    if let Some(open) = cx.try_global::<OpenWindow>().map(|open| open.handle)
        && cx
            .update_window(open, |_, window, _| window.activate_window())
            .is_ok()
    {
        return Ok(());
    }
    open(cx).map(|_| ())
}

pub(crate) fn watch_waiting(cx: &mut App) {
    attention::init(cx);
    #[cfg(target_os = "macos")]
    dock::watch(cx);
    notify::init(cx);
    notify::watch(cx);
}

pub(super) fn shows(id: &ThreadId, cx: &App) -> bool {
    cx.try_global::<OpenWindow>()
        .and_then(|open| open.kagan.upgrade())
        .is_some_and(|kagan| kagan.read(cx).shown_thread(cx).as_ref() == Some(id))
}

/// Shows the settings page in the window, opening the window first when none is open.
pub(crate) fn front(cx: &App) -> Option<(AnyWindowHandle, Entity<Kagan>)> {
    let open = cx.try_global::<OpenWindow>()?;
    Some((open.handle, open.kagan.upgrade()?))
}

pub(crate) fn show_settings(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    let Some(open) = cx.try_global::<OpenWindow>() else {
        return Ok(());
    };
    let (handle, kagan) = (open.handle, open.kagan.clone());
    if let Some(kagan) = kagan.upgrade() {
        _ = cx.update_window(handle, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.open_settings(window, cx));
        });
    }
    Ok(())
}

pub(crate) fn show_plugin_settings(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    if let Some((handle, kagan)) = front(cx) {
        _ = cx.update_window(handle, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.manage_plugins(window, cx));
        });
    }
    Ok(())
}

pub(crate) fn start_plugin(cx: &mut App) -> gpui_kit::Result<()> {
    show(cx)?;
    if let Some((handle, kagan)) = front(cx) {
        _ = cx.update_window(handle, |_, window, cx| {
            kagan.update(cx, |kagan, cx| kagan.add_plugin(window, cx));
        });
    }
    Ok(())
}

fn release_window(this: &mut Kagan, cx: &mut App) {
    web_panel::clear(cx);
    let id = this.window.window_id();
    this.cancel_plugin_start();
    this.surface_check = None;
    this.surface_hold = false;
    if let Some(host) = HostUi::global(cx) {
        host.update(cx, |host, cx| host.remove_window(id, cx));
    }
}

pub(crate) fn window_options(cx: &mut App) -> WindowOptions {
    crate::prepare_window(WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(gpui_kit::Bounds::centered(
            None,
            size(px(1120.), px(720.)),
            cx,
        ))),
        titlebar: Some(gpui_kit::TitlebarOptions {
            title: Some(TITLE.into()),
            ..TitleBar::title_bar_options()
        }),
        window_min_size: Some(size(
            px(layout::CENTER_MIN + layout::RAIL_WIDTH),
            px(MIN_HEIGHT),
        )),
        ..TitleBar::window_options()
    })
}

impl Kagan {
    fn bigger(&mut self, _: &Bigger, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_size(self.terminal_font_size.saturating_add(1), cx);
    }

    fn smaller(&mut self, _: &Smaller, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_size(self.terminal_font_size.saturating_sub(1), cx);
    }

    fn actual_size(&mut self, _: &ActualSize, _: &mut Window, cx: &mut Context<Self>) {
        self.change_terminal_size(12, cx);
    }

    fn theme_paint(cx: &mut Context<Self>) -> Subscription {
        cx.observe(&crate::theme::ThemeState::global(cx), |_, _, cx| {
            web_panel::repaint(cx);
            cx.notify();
        })
    }

    fn theme_notice(cx: &mut Context<Self>) -> Subscription {
        let theme = crate::theme::ThemeState::global(cx);
        cx.subscribe(&theme, |this, _, notice: &crate::theme::Notice, cx| {
            this.show_toast(Toast::said(notice.0.clone()), cx);
        })
    }

    fn open_answer(op: Op, window: &mut Window, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(op, cx);
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| match result {
                Ok(OpResult::Files(paths)) => {
                    for url in paths
                        .iter()
                        .filter_map(|path| url::Url::from_file_path(path).ok())
                    {
                        cx.open_url(url.as_str());
                    }
                }
                Ok(_) => {}
                Err(error) => this.show_toast(Toast::said(error.to_string()), cx),
            });
        })
        .detach();
    }

    fn apply_terminal_size(&mut self, size: u8, cx: &mut Context<Self>) {
        self.terminal_font_size = size;
        if let Shown::Thread(view) = self.screen.shown() {
            view.update(cx, |view, cx| view.set_font_size(size, cx));
        }
        self.tabs.set_font_size(size, cx);
    }

    fn change_terminal_size(&mut self, size: u8, cx: &mut Context<Self>) {
        let size = size.clamp(9, 24);
        self.terminal_font_epoch = self.terminal_font_epoch.wrapping_add(1);
        self.apply_terminal_size(size, cx);
        layout::save("terminal-font-size", size, cx);
    }

    fn load_terminal_size(&mut self, cx: &mut Context<Self>) {
        self.terminal_font_epoch = self.terminal_font_epoch.wrapping_add(1);
        let epoch = self.terminal_font_epoch;
        let read = layout::read::<u8>("terminal-font-size", cx);
        cx.spawn(async move |this, cx| {
            let size = read.await.ok().flatten().unwrap_or(12).clamp(9, 24);
            _ = this.update(cx, |this, cx| {
                if this.terminal_font_epoch == epoch {
                    this.apply_terminal_size(size, cx);
                }
            });
        })
        .detach();
    }

    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = Workspace::global(cx);
        let picker = cx.new(|cx| picker::Picker::new(window, cx));
        let columns = cx.new(|_| ResizableState::default());
        let prefs = settings::state(cx);
        let archive_query =
            cx.new(|cx| InputState::new(window, cx).placeholder("Search archived threads"));
        let subscriptions = listen(&workspace, &picker, &columns, window, cx);
        let sidebar = cx.new(|cx| sidebar::Sidebar::new(workspace.read(cx).sidebar(cx), cx));
        let sidebar_watch = watch_sidebar(&sidebar, window, cx);
        let mut this = Self {
            start: start::StartState::default(),
            screen: screen::Screen::new(),
            terminal_font_size: 12,
            terminal_font_epoch: 0,
            window_title: TITLE.to_owned(),
            sidebar,
            picker,
            questions: attention::global(cx),
            _questions_watch: cx.observe_in(
                &attention::global(cx),
                window,
                |this, _, window, cx| {
                    this.announce_questions(window, cx);
                    cx.notify();
                },
            ),
            note: None,
            plugin_prompt_notice: None,
            focus: cx.focus_handle(),
            surface_close_focus: cx.focus_handle().tab_stop(true),
            surface_hold: false,
            typing: false,
            quiet: Task::ready(()),
            on_screen: BTreeSet::new(),
            announced: BTreeSet::new(),
            typed_watch: None,
            changes: Changes::new(),
            details: details::Details::default(),
            tabs: tabs::Tabs::default(),
            review_drafts: review::Drafts::default(),
            replaced: replace::Replaced::default(),
            plugins: Plugins::new(window, cx),
            panels: right::built_in(),
            right_slots: Vec::new(),
            right_tab: None,
            right_menu: None,
            right_add_focus: cx.focus_handle().tab_stop(true),
            right_fill: right::Fill::Column,
            right_panels: Vec::new(),
            tool: None,
            surface_check: None,
            surface_epoch: 0,
            layout: Layout::default(),
            viewport: window.viewport_size().width,
            prefs: prefs.clone(),
            _settings_watch: settings::watch(&prefs, cx),
            panes: layout::Panes {
                left: true,
                right: true,
            },
            right_toggle_focus: cx.focus_handle().tab_stop(true),
            left_overlay_focus: cx.focus_handle().tab_stop(true),
            start_on_load: None,
            settings_scroll: ScrollHandle::new(),
            settings_approvals: 0,
            settings_drafts: settings::Drafts::default(),
            keyboard: settings::Recorder::default(),
            settings_views: BTreeMap::new(),
            settling: false,
            columns,
            window: window.window_handle(),
            actions_menu: None,
            open_thread: None,
            retired_load: Task::ready(()),
            _archive_watch: archived::watch(&archive_query, cx),
            archive_query,
            browser: browser::Browser::new(window, cx),
            title_input: None,
            title_watch: None,
            _subscriptions: subscriptions,
            _sidebar_watch: sidebar_watch,
            _appearance: crate::theme::watch_system(window, cx),
            _theme_paint: Self::theme_paint(cx),
            _theme_notice: Self::theme_notice(cx),
            plugin_watch: None,
            host_events: Vec::new(),
            overlays: Vec::new(),
            ask_drafts: BTreeMap::new(),
            ask_hidden: BTreeSet::new(),
            ask_popup: None,
            toast: None,
            toast_epoch: 0,
            _release: cx.on_release(release_window),
            _poll: changes::poll_open_changes(cx),
            commands: None,
        };
        this.load_settings(cx);
        this.refresh(window, cx);
        this.show_newest(window, cx);
        this
    }

    fn show_newest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let newest = self.listed(cx).next().map(|thread| thread.id.clone());
        match newest {
            Some(newest) => self.select(newest, window, cx),
            None => window.focus(&self.focus, cx),
        }
    }

    fn left_side(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let shows = self.shows(cx);
        let tool = surface::tool_region(self, window, cx);
        let threads = self.replaced_body(kg_domain::Slot::Threads, window, cx);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar::list(sidebar, &shows, threads, tool, window, cx)
        })
    }

    fn rail_side(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let shows = self.shows(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar::rail(sidebar, &shows, window, cx))
    }

    fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let approvals = Workspace::global(cx).read(cx).approvals().count();
        if self.in_settings() && approvals > self.settings_approvals {
            self.settings_scroll.scroll_to_top_of_item(1);
        }
        self.settings_approvals = approvals;
        self.announce_questions(window, cx);
        let listing = Workspace::global(cx).read(cx).sidebar(cx);
        let footer = sidebar::archived_footer(&Workspace::global(cx).read(cx).thread_records(cx));
        self.sidebar
            .update(cx, |sidebar, _| sidebar.replace(listing, footer));
        self.wake_threads_plugin(cx);
        self.note_shown(cx);
        // A shown thread that was deleted leaves no terminal to hold the keyboard, so the window does.
        let listed = |id: &ProjectId| self.sidebar.read(cx).project(id).is_some();
        let gone = match self.screen.shown() {
            Shown::Thread(view) => self.record(view.read(cx).thread(), cx).is_none(),
            Shown::Start(id) => !listed(id),
            Shown::Nothing => false,
        };
        if self.sidebar.read(cx).chosen().is_some_and(|id| !listed(id)) {
            self.sidebar.update(cx, |sidebar, _| sidebar.choose(None));
        }
        if gone {
            self.cancel_plugin_start();
            self.show_work(Shown::Nothing, window, cx);
            self.dismiss_thread_menus(cx);
            window.focus(&self.focus, cx);
        }
        self.watch_plugins(window, cx);
        if self.shows_archived() {
            self.load_retired(cx);
        }
        self.load_changes(cx);
        self.load_details(cx);
        self.sync_tabs(window, cx);
        self.refresh_plugins(cx);
        self.tell_host(window, cx);
        cx.notify();
    }

    /// Watches the plugin host, so a plugin turned off or updated changes its panel at once.
    fn watch_plugins(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // The plugin host starts after the UI's own setup, so the key bindings for its commands are
        // installed here, once the host exists. Installing them again does nothing.
        crate::plugins::init(cx);
        if self.plugin_watch.is_some() {
            return;
        }
        let Some(host) = kg_plugins::PluginHost::try_global(cx) else {
            return;
        };
        self.plugin_watch = Some(cx.observe_in(&host, window, |this, _, window, cx| {
            this.sync_slots(window, cx);
            this.sync_replaced(window, cx);
            this.reconcile_plugin_views(window, cx);
            this.sync_overlays(window, cx);
            cx.notify();
        }));
        self.sync_overlays(window, cx);
        if let Some(ui) = HostUi::global(cx) {
            self.host_events.push(cx.subscribe_in(
                &ui,
                window,
                |this, _, event: &HostUiEvent, window, cx| this.host_ui(event, window, cx),
            ));
        }
        self.host_events.push(cx.subscribe_in(
            &host,
            window,
            |this, _, event: &PluginHostEvent, window, cx| {
                this.plugin_host_event(event, window, cx);
            },
        ));
    }

    /// Tells the plugin host which thread and project the window shows. Plugin panels that follow
    /// the thread read it from there.
    fn tell_host(&self, window: &Window, cx: &mut App) {
        let Some(host) = HostUi::global(cx) else {
            return;
        };
        let thread = self.shown_thread(cx);
        let project = self.shown_project(cx);
        let follows: Vec<PluginId> = kg_plugins::contributions(cx)
            .map(|list| {
                list.into_iter()
                    .filter(|(_, ui)| {
                        ui.panels
                            .iter()
                            .any(|panel| matches!(panel.follows, Some(Follows::Thread)))
                    })
                    .map(|(plugin, _)| plugin)
                    .collect()
            })
            .unwrap_or_default();
        let handle = window.window_handle();
        let focused = window.is_window_active();
        host.update(cx, |host, cx| {
            host.set_context(handle, thread, project, focused, &follows, cx);
        });
    }

    /// Threads on the sidebar, newest first.
    fn listed<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = &'a Thread> {
        self.sidebar
            .read(cx)
            .listing
            .threads
            .iter()
            .filter(|thread| thread.archived_at.is_none())
    }

    /// The thread on screen. The settings page covers the terminal, so none is shown then.
    pub(crate) fn shown_thread(&self, cx: &App) -> Option<ThreadId> {
        match self.screen.shown() {
            Shown::Thread(view) if !self.in_settings() => Some(view.read(cx).thread().clone()),
            Shown::Thread(_) | Shown::Nothing | Shown::Start(_) => None,
        }
    }

    /// The project on screen: the shown thread's, or the project a new thread would start in. The
    /// settings page covers the window, so no project is on screen then.
    pub(crate) fn shown_project(&self, cx: &App) -> Option<ProjectId> {
        if self.in_settings() {
            return None;
        }
        match (self.shown_thread(cx), self.screen.shown()) {
            (Some(id), _) => self.record(&id, cx).map(|thread| thread.project_id.clone()),
            (None, Shown::Start(project)) => Some(project.clone()),
            (None, Shown::Nothing | Shown::Thread(_)) => None,
        }
    }

    /// A plugin asked the window for something: a thread, a panel, or a toast. Each request gets
    /// one reply, so the plugin's call returns the result instead of waiting on a dropped sender.
    fn host_ui(&mut self, event: &HostUiEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            HostUiEvent::OpenThread { id, reply } => {
                self.select(id.clone(), window, cx);
                let _ = reply.try_send(Ok(()));
            }
            HostUiEvent::OpenPanel {
                plugin,
                panel,
                props,
                reply,
            } => {
                let reply = reply.clone();
                let step = self.commit(
                    SurfaceOp::Open {
                        plugin: plugin.clone(),
                        id: panel.clone(),
                    },
                    Some(props.clone()),
                    window,
                    cx,
                );
                match step {
                    Ok(SurfaceCommit::Ready(_)) => {
                        let _ = reply.try_send(Ok(()));
                    }
                    Ok(SurfaceCommit::Paint) => {
                        let kagan = cx.entity();
                        surface::schedule_settle(kagan, window, cx, move |settled| {
                            let _ = reply
                                .try_send(settled.map(|_| ()).map_err(|error| error.to_string()));
                        });
                    }
                    Err(error) => {
                        let _ = reply.try_send(Err(error.to_string()));
                    }
                }
            }
            HostUiEvent::Toast {
                plugin,
                message,
                action,
                reply,
            } => {
                self.show_toast(
                    Toast {
                        message: message.clone().into(),
                        action: action.as_ref().map(|action| ToastCommand {
                            plugin: plugin.clone(),
                            command: action.command.clone(),
                            label: action.label.clone().into(),
                        }),
                        undo: Vec::new(),
                    },
                    cx,
                );
                let _ = reply.try_send(Ok(()));
            }
            HostUiEvent::Comment {
                plugin,
                comment,
                reply,
            } => {
                let window = window.window_handle().window_id();
                if let Some(saved) = self.plugin_comment(plugin, comment, window, cx) {
                    let _ = reply.try_send(saved);
                }
            }
            HostUiEvent::PostPage {
                plugin,
                panel,
                message,
                reply,
            } => {
                let _ = reply.try_send(web_panel::post(plugin, panel, message, cx));
            }
        }
    }

    /// A plugin's own notification opens its thread. File views are not shown in this window, so
    /// a request for one gets an answer rather than a wait.
    fn plugin_host_event(
        &mut self,
        event: &PluginHostEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PluginHostEvent::OpenThread(id) => {
                let visible = Workspace::global(cx)
                    .read(cx)
                    .thread(id)
                    .is_some_and(|thread| {
                        thread.read(cx).record().visibility == kg_domain::Visibility::Visible
                    });
                if visible {
                    self.select(id.clone(), window, cx);
                }
            }
        }
    }

    /// A plugin command bound to a key or a toast. It runs in the thread and project on screen.
    fn run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = PluginHost::try_global(cx) else {
            return;
        };
        let task = PluginHost::dispatch_command(
            &host,
            action.plugin(),
            action.command(),
            self.shown_thread(cx),
            self.shown_project(cx),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            if let Err(error) = task.await {
                _ = this.update(cx, |this, cx| {
                    this.note = Some(error.to_string().into());
                    cx.notify();
                });
            }
        })
        .detach();
    }

    fn show_toast(&mut self, toast: Toast, cx: &mut Context<Self>) {
        self.toast_epoch = self.toast_epoch.wrapping_add(1);
        let epoch = self.toast_epoch;
        let stays = toast.action.is_some();
        let pause = if toast.undo.is_empty() {
            std::time::Duration::from_secs(5)
        } else {
            std::time::Duration::from_secs(10)
        };
        self.toast = Some(toast);
        cx.notify();
        if stays {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(pause).await;
            _ = this.update(cx, |this, cx| {
                if this.toast_epoch == epoch {
                    this.toast = None;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn render_toast(toast: &Toast, cx: &mut Context<Self>) -> impl IntoElement {
        let message = toast.message.clone();
        let ids = toast.undo.clone();
        let undo = (!ids.is_empty()).then(|| {
            toast_button(
                "toast-undo",
                "Undo".into(),
                cx.listener(move |this, _, window, cx| this.undo_archive(ids.clone(), window, cx)),
            )
        });
        let action = toast.action.clone().map(|action| {
            let run = RunCommand::new(action.plugin.clone(), action.command.clone());
            toast_button(
                "toast-action",
                action.label.clone(),
                move |_, window, cx| window.dispatch_action(Box::new(run.clone()), cx),
            )
        });
        h_flex()
            .id("toast")
            .ui_container()
            .absolute()
            .left_4()
            .right_4()
            .bottom_4()
            .min_w_0()
            .bg(cx.theme().popover)
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .px_4()
            .py_2()
            .gap_3()
            .type_body()
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .id("toast-message")
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .text_ellipsis()
                    .tooltip({
                        let message = message.clone();
                        move |window, cx| Tooltip::new(message.clone()).build(window, cx)
                    })
                    .role(gpui_kit::Role::Status)
                    .aria_label(message.clone())
                    .child(message.clone())
                    .ui_observe_as(Facts {
                        role: gpui_kit::Role::Status,
                        value: Some(message.clone()),
                        ..Facts::button(message)
                    }),
            )
            .children(undo)
            .children(action)
            .child(toast_button(
                "toast-dismiss",
                "Dismiss".into(),
                cx.listener(|this, _, _, cx| {
                    this.toast = None;
                    cx.notify();
                }),
            ))
    }

    #[cfg(test)]
    pub(crate) fn question_titles(&self, cx: &App) -> Vec<String> {
        self.questions
            .read(cx)
            .questions
            .iter()
            .map(|asked| asked.title.clone())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn right_open(&self) -> bool {
        self.layout.right_open
    }

    #[cfg(test)]
    pub(crate) fn left_open(&self) -> bool {
        self.layout.left_open
    }

    /// Drops one panel from the registry, so a test can show that the sidebar draws from it.
    #[cfg(test)]
    pub(crate) fn drop_panel_for_test(&mut self, id: &str, cx: &mut Context<Self>) {
        self.panels.retain(|panel| panel.id != id);
        cx.notify();
    }

    /// The text a plugin's text setting shows on its page, for the tests.
    #[cfg(test)]
    pub(crate) fn settings_text(cx: &App, plugin: &kg_domain::PluginId, key: &str) -> String {
        settings::text_for_test(plugin, key, cx)
    }

    #[cfg(test)]
    pub(crate) fn settings_view_mounted(&self, id: &kg_domain::PluginId) -> bool {
        self.settings_views.contains_key(id)
    }

    /// The load generation a mounted settings view was made from, for the tests.
    #[cfg(test)]
    pub(crate) fn settings_view_generation(&self, id: &kg_domain::PluginId) -> Option<u64> {
        self.settings_views
            .get(id)
            .map(|(generation, _)| *generation)
    }

    #[cfg(test)]
    pub(crate) fn agent_integration(&self, cx: &App) -> bool {
        self.prefs.read(cx).agent_integration
    }

    /// Every registered right-sidebar panel's title, in order.
    #[cfg(test)]
    pub(crate) fn panel_titles(&self) -> Vec<String> {
        self.panels
            .iter()
            .map(|panel| panel.title.clone())
            .collect()
    }

    /// The width of the right panel as the group holds it now.
    #[cfg(test)]
    pub(crate) fn right_width_now(&self, cx: &App) -> f32 {
        self.columns
            .read(cx)
            .sizes()
            .get(layout::right_index(self.panes.left))
            .map_or(0., |width| f32::from(*width))
    }

    #[cfg(test)]
    pub(crate) fn resize_left(&self, width: gpui_kit::Pixels, window: &mut Window, cx: &mut App) {
        self.columns
            .update(cx, |state, cx| state.resize_panel(0, width, window, cx));
    }

    #[cfg(test)]
    pub(crate) fn resize_right(&self, width: gpui_kit::Pixels, window: &mut Window, cx: &mut App) {
        self.columns.update(cx, |state, cx| {
            state.resize_panel(layout::right_index(self.panes.left), width, window, cx);
        });
    }

    #[cfg(test)]
    pub(crate) fn picker_entity(&self) -> Entity<picker::Picker> {
        self.picker.clone()
    }

    fn record<'a>(&'a self, id: &ThreadId, cx: &'a App) -> Option<&'a Thread> {
        self.sidebar
            .read(cx)
            .listing
            .threads
            .iter()
            .find(|thread| thread.id == *id)
            .or(self.open_thread.as_ref().filter(|thread| thread.id == *id))
    }

    fn note_shown(&mut self, cx: &App) {
        let Some(id) = (match self.screen.shown() {
            Shown::Thread(view) => Some(view.read(cx).thread().clone()),
            Shown::Nothing | Shown::Start(_) => None,
        }) else {
            self.open_thread = None;
            return;
        };
        if self
            .sidebar
            .read(cx)
            .listing
            .threads
            .iter()
            .any(|thread| thread.id == id)
        {
            self.open_thread = None;
            return;
        }
        self.open_thread = Workspace::global(cx)
            .read(cx)
            .thread(&id)
            .map(|thread| thread.read(cx).record().clone())
            .filter(|thread| thread.visibility == kg_domain::Visibility::Visible);
    }

    fn waited(&mut self, id: &ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.screen.current(), screen::Center::Work(Shown::Thread(view)) if view.read(cx).thread() == id)
        {
            Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx).detach();
        } else {
            self.refresh(window, cx);
        }
    }

    fn read_shown(&self, cx: &mut Context<Self>) {
        if let screen::Center::Work(Shown::Thread(view)) = self.screen.current() {
            let id = view.read(cx).thread().clone();
            if Workspace::global(cx)
                .read(cx)
                .thread(&id)
                .is_some_and(|thread| thread.read(cx).record().mark == kg_domain::ThreadMark::Done)
            {
                Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id }), cx).detach();
            }
        }
    }

    pub(crate) fn select(&mut self, id: ThreadId, window: &mut Window, cx: &mut Context<Self>) {
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal_thread(&id, cx));
        self.start_on_load = None;
        if self.start.plugin_thread.as_ref() != Some(&id) {
            self.cancel_plugin_start();
        }
        let view = cx.new(|cx| TerminalView::new(id.clone(), None, self.terminal_font_size, cx));
        self.show_work(Shown::Thread(view.clone()), window, cx);
        self.focus_work(window, cx);
        self.typed_watch = Some(cx.subscribe(&view, |this, _, _: &Typed, cx| this.typed(cx)));
        self.sidebar.update(cx, |sidebar, _| sidebar.choose(None));
        self.load_changes(cx);
        self.load_details(cx);
        self.sync_tabs(window, cx);
        self.tell_host(window, cx);
        self.note_shown(cx);
        Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id }), cx).detach();
        cx.notify();
    }

    fn show_start(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_plugin_start();
        self.start_on_load = None;
        self.show_work(Shown::Start(project.clone()), window, cx);
        self.sidebar
            .update(cx, |sidebar, _| sidebar.choose(Some(project.clone())));
        self.start.reset();
        self.load_branches(project, cx);
        self.picker.update(cx, |picker, cx| picker.load(window, cx));
        self.picker
            .update(cx, |picker, cx| picker.focus(window, cx));
        self.tell_host(window, cx);
        cx.notify();
    }

    /// Runs a thread operation; a thread it returns is shown, a failure is said.
    fn run(op: ThreadOp, action: &'static str, window: &Window, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(Op::Thread(op), cx);
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| this.finish(result, action, window, cx));
            });
        })
        .detach();
    }

    fn finish(
        &mut self,
        result: Result<OpResult, OpError>,
        action: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if result.is_ok() {
            self.note = None;
        }
        match result {
            Ok(OpResult::Thread(thread)) => self.select(thread.id, window, cx),
            Ok(_) => self.refresh(window, cx),
            Err(error) => {
                self.note = Some(format!("Could not {action}: {error}").into());
                cx.notify();
            }
        }
    }

    fn new_thread(&mut self, _: &NewThread, window: &mut Window, cx: &mut Context<Self>) {
        let project = self
            .sidebar
            .read(cx)
            .chosen()
            .cloned()
            .or_else(|| {
                self.shown_thread(cx)
                    .and_then(|id| self.record(&id, cx).map(|thread| thread.project_id.clone()))
            })
            .or_else(|| {
                self.sidebar
                    .read(cx)
                    .listing
                    .projects
                    .first()
                    .map(|project| project.id.clone())
            });
        if let Some(project) = project {
            self.show_start(project, window, cx);
        }
    }

    fn stop(&mut self, _: &StopThread, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.shown_thread(cx) {
            Self::run(ThreadOp::Stop { id }, "stop the thread", window, cx);
        }
    }

    fn save_diagnostics(
        _: &mut Self,
        _: &SaveDiagnostics,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = Workspace::dispatch(
            kg_domain::Op::Diagnostics(kg_domain::DiagnosticsOp::Export { path: None }),
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let result = task.await;
            _ = this.update(cx, |this, cx| {
                match result {
                    Ok(kg_domain::OpResult::Diagnostics { path }) => {
                        this.note = None;
                        this.show_toast(
                            Toast {
                                message: format!("Saved diagnostics to {}", path.display()).into(),
                                action: None,
                                undo: Vec::new(),
                            },
                            cx,
                        );
                    }
                    Err(error) => {
                        this.note = Some(format!("Could not save diagnostics: {error}").into());
                    }
                    Ok(_) => {}
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn start_again(&mut self, _: &StartAgain, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.shown_thread(cx) {
            Self::run(
                ThreadOp::StartAgain { id },
                "start the thread again",
                window,
                cx,
            );
        }
    }

    fn toggle_pinned(&mut self, _: &TogglePinned, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self
            .shown_thread(cx)
            .and_then(|id| self.record(&id, cx).cloned())
        else {
            return;
        };
        let pin = ThreadOp::SetPinned {
            id: thread.id,
            pinned: !thread.pinned,
        };
        Self::run(pin, "change the pin", window, cx);
    }

    pub(crate) fn restore(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.shown_thread(cx) {
            Self::run(ThreadOp::Unarchive { id }, "restore the thread", window, cx);
        }
    }

    fn restore_thread(&mut self, _: &RestoreThread, window: &mut Window, cx: &mut Context<Self>) {
        self.restore(window, cx);
    }

    fn archive(&mut self, _: &ArchiveThread, window: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self
            .shown_thread(cx)
            .and_then(|id| self.record(&id, cx).cloned())
        else {
            return;
        };
        let confirm = Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: "confirm-archive".into(),
            }),
            cx,
        );
        let text = description(
            thread.status,
            archive_span(&self.sidebar.read(cx).listing.threads, &thread.id),
        );
        let handle = window.window_handle();
        cx.spawn(async move |this, cx| {
            let ask = matches!(
                confirm.await,
                Ok(OpResult::Setting(serde_json::Value::Bool(true)))
            );
            _ = cx.update_window(handle, |_, window, cx| {
                if ask {
                    confirm_archive(this, thread, text, window, cx);
                } else {
                    _ = this.update(cx, |this, cx| this.archive_now(&thread, window, cx));
                }
            });
        })
        .detach();
    }

    fn archive_now(&mut self, thread: &Thread, window: &mut Window, cx: &mut Context<Self>) {
        let id = thread.id.clone();
        let leaving = archived_with(&Workspace::global(cx).read(cx).thread_records(cx), &id);
        let next = self.next_thread(&id, &leaving, cx);
        self.dismiss_thread_menus(cx);
        self.close_rename();
        if let Some(next) = next {
            self.select(next, window, cx);
        } else {
            self.cancel_plugin_start();
            self.show_work(Shown::Nothing, window, cx);
            window.focus(&self.focus, cx);
        }
        self.show_toast(
            Toast {
                message: format!("Archived “{}”", thread.title).into(),
                action: None,
                undo: leaving,
            },
            cx,
        );
        Self::run(ThreadOp::Archive { id }, "archive the thread", window, cx);
    }

    fn undo_archive(&mut self, ids: Vec<ThreadId>, window: &mut Window, cx: &mut Context<Self>) {
        self.toast = None;
        if let Some(id) = ids.first() {
            self.select(id.clone(), window, cx);
        }
        for id in ids {
            Self::run(ThreadOp::Unarchive { id }, "restore the thread", window, cx);
        }
    }

    fn next_thread(&self, id: &ThreadId, leaving: &[ThreadId], cx: &App) -> Option<ThreadId> {
        let leaving: BTreeSet<&ThreadId> = leaving.iter().collect();
        let rows = self.sidebar.read(cx).thread_ids(&self.shows(cx), cx);
        let stays = |row: &ThreadId| !leaving.contains(row);
        let neighbor = rows.iter().position(|row| row == id).and_then(|index| {
            rows.iter()
                .skip(index + 1)
                .find(|row| stays(row))
                .or_else(|| rows.iter().take(index).rev().find(|row| stays(row)))
                .cloned()
        });
        neighbor.or_else(|| {
            self.listed(cx)
                .find(|thread| stays(&thread.id))
                .map(|thread| thread.id.clone())
        })
    }

    fn pick_project(window: &mut Window, cx: &mut Context<Self>) {
        let picked = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add project".into()),
        });
        let window = window.window_handle();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = picked.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |_, cx| Self::add_picked_project(path, window, cx));
            });
        })
        .detach();
    }
}

// Unarchive fails while the owner is still archived, so the owner is listed first.
fn archived_with(threads: &[Thread], id: &ThreadId) -> Vec<ThreadId> {
    let mut ids = vec![id.clone()];
    for owned in kg_domain::owned_thread_ids(threads, id).into_iter().skip(1) {
        if threads
            .iter()
            .find(|thread| thread.id == owned)
            .is_some_and(|thread| thread.archived_at.is_none())
        {
            ids.push(owned);
        }
    }
    ids
}

fn confirm_archive(
    kagan: gpui_kit::WeakEntity<Kagan>,
    thread: Thread,
    text: String,
    window: &mut Window,
    cx: &mut App,
) {
    window.open_alert_dialog(cx, move |dialog, _, _| {
        let kagan = kagan.clone();
        let archived = thread.clone();
        dialog
            .title(format!("Archive “{}”?", thread.title))
            .description(text.clone())
            .ok_text("Archive")
            .on_ok(move |_, window, cx| {
                _ = kagan.update(cx, |kagan, cx| {
                    kagan.archive_now(&archived, window, cx);
                });
                true
            })
    });
}

pub(crate) fn description(status: ThreadStatus, extra: usize) -> String {
    let mut sentences = Vec::new();
    if matches!(
        status,
        ThreadStatus::Starting | ThreadStatus::Active | ThreadStatus::Stopping
    ) {
        sentences.push("This will stop current work.".to_owned());
    }
    sentences.extend(match extra {
        0 => None,
        1 => Some("1 thread that lives with this one will be archived with it.".to_owned()),
        _ => Some(format!(
            "{extra} threads that live with this one will be archived with it."
        )),
    });
    sentences.push("It leaves the sidebar. Restore it from Archived.".to_owned());
    sentences.join(" ")
}

impl Kagan {
    /// Plugin views that are in this frame, so a native page can hide with its panel.
    fn shown_plugin_views(&self, cx: &App) -> Vec<gpui_kit::EntityId> {
        let mut ids = Vec::new();
        let full = self.right_fill == right::Fill::Window && self.panes.right;
        if self.right_drawn()
            && let Some(tab) = right::selected_tab(self, cx)
            && let Some(view) = self
                .right_slots
                .iter()
                .find(|slot| slot.id == tab)
                .and_then(|slot| slot.view.as_ref())
        {
            ids.push(view.entity_id());
        }
        let left = !full
            && (self.panes.left || (self.screen.drawn(screen::Side::Left) && !self.in_settings()));
        if left && let Some(view) = self.tool.as_ref().and_then(|slot| slot.view.as_ref()) {
            ids.push(view.entity_id());
        }
        if left && let Some(view) = self.replaced.mounted_view(kg_domain::Slot::Threads) {
            ids.push(view.entity_id());
        }
        if !full {
            match self.screen.current() {
                screen::Center::Page(page) => {
                    if let Some(view) = page.view.as_ref() {
                        ids.push(view.entity_id());
                    }
                }
                screen::Center::Review(_) => {
                    if let Some(view) = self.replaced.mounted_view(kg_domain::Slot::Review) {
                        ids.push(view.entity_id());
                    }
                }
                screen::Center::Settings => {
                    for (_, view) in self.settings_views.values() {
                        ids.push(view.entity_id());
                    }
                }
                screen::Center::Work(_) | screen::Center::Archived(_) => {}
            }
        }
        ids
    }
}

impl Render for Kagan {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main = self.center_body(window, cx);
        self.fit_to_window(window, cx);
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.sync_browser(cx);
        crate::components::sync_pages(&self.shown_plugin_views(cx), cx);
        web_panel::sync(self, cx);
        let columns = if self.right_fill == right::Fill::Window && self.panes.right {
            div()
                .id("right-full-column")
                .size_full()
                .child(right::render(self, window, cx))
                .into_any_element()
        } else {
            self.columns(main, window, cx)
        };
        v_flex()
            .id("kagan")
            .type_body()
            .key_context(window_context(self, cx).as_str())
            .track_focus(&self.focus)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::toggle_details_action))
            .on_action(cx.listener(Self::toggle_fill_action))
            .on_action(cx.listener(Self::toggle_form_action))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::new_terminal))
            .on_action(cx.listener(Self::close_terminal))
            .on_action(cx.listener(Self::start_again))
            .on_action(cx.listener(Self::save_diagnostics))
            .on_action(cx.listener(Self::run_command))
            .on_action(cx.listener(Self::toggle_pinned))
            .on_action(cx.listener(Self::archive))
            .on_action(cx.listener(|_, _: &AddProject, window, cx| Self::pick_project(window, cx)))
            .on_action(cx.listener(Self::focus_sidebar))
            .on_action(cx.listener(Self::restore_thread))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_right))
            .on_action(cx.listener(Self::bigger))
            .on_action(cx.listener(Self::smaller))
            .on_action(cx.listener(Self::actual_size))
            .on_action(cx.listener(Self::use_built_in_sidebar))
            .on_action(cx.listener(Self::use_built_in_review))
            .on_action(cx.listener(|_, _: &NewTheme, window, cx| {
                let id = crate::theme::copy_id(cx);
                Self::open_answer(Op::Theme(ThemeOp::New { id, from: None }), window, cx);
            }))
            .on_action(cx.listener(|_, _: &OpenThemes, window, cx| {
                Self::open_answer(Op::Theme(ThemeOp::Folder), window, cx);
            }))
            .on_action(cx.listener(|this, _: &DismissRightOverlay, window, cx| {
                this.dismiss_overlay(screen::Side::Right, window, cx);
            }))
            .on_action(cx.listener(|this, _: &DismissLeftOverlay, window, cx| {
                this.dismiss_overlay(screen::Side::Left, window, cx);
            }))
            .on_action(cx.listener(Self::toggle_settings))
            .on_action(cx.listener(Self::use_project_folder))
            .on_action(cx.listener(Self::use_worktree_folder))
            .on_action(cx.listener(Self::retry_branches_action))
            .on_action(cx.listener(Self::retry_programs))
            .on_action(cx.listener(Self::toggle_custom_command))
            .on_action(cx.listener(Self::open_thread_actions))
            .on_action(cx.listener(Self::begin_rename))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::close_page))
            .on_action(commands::open_list)
            .on_action(commands::search_threads)
            .on_action(|_: &settings::NextItem, window, cx| window.focus_next(cx))
            .on_action(|_: &settings::PreviousItem, window, cx| window.focus_prev(cx))
            .child(self.title_bar(window, cx))
            .child(window_body(self, columns, window, cx))
    }
}

fn window_body(
    kagan: &mut Kagan,
    columns: impl IntoElement,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> impl IntoElement {
    h_flex()
        .flex_1()
        .min_h_0()
        .relative()
        .child(kagan.rail_side(window, cx))
        .child(div().flex_1().min_w_0().h_full().child(columns))
        .children(kagan.left_layer(window, cx))
        .children(kagan.right_layer(window, cx))
        .children(commands::layer(kagan, window, cx))
}

fn watch_sidebar(
    sidebar: &Entity<sidebar::Sidebar>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> [Subscription; 2] {
    [
        cx.observe(sidebar, |_, _, cx| cx.notify()),
        cx.subscribe_in(
            sidebar,
            window,
            |this, _, event: &sidebar::Event, window, cx| {
                this.sidebar_event(event, window, cx);
            },
        ),
    ]
}

fn listen(
    workspace: &Entity<Workspace>,
    picker: &Entity<picker::Picker>,
    columns: &Entity<ResizableState>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> [Subscription; 5] {
    [
        cx.subscribe_in(
            workspace,
            window,
            |this, _, event, window, cx| match event {
                WorkspaceEvent::Changed => {
                    this.refresh(window, cx);
                    this.load_settings(cx);
                }
                WorkspaceEvent::Event { .. } => this.refresh(window, cx),
                WorkspaceEvent::Waiting(id) | WorkspaceEvent::Finished(id) => {
                    this.waited(id, window, cx);
                }
                WorkspaceEvent::Open
                | WorkspaceEvent::Settings
                | WorkspaceEvent::SettingsPlugins
                | WorkspaceEvent::NewPlugin
                | WorkspaceEvent::Window(_)
                | WorkspaceEvent::Screen(..) => {}
            },
        ),
        cx.subscribe_in(picker, window, |this, _, _: &picker::Start, window, cx| {
            this.start(window, cx);
        }),
        cx.subscribe_in(picker, window, |this, _, _: &picker::Close, window, cx| {
            this.show_work(Shown::Nothing, window, cx);
            window.focus(&this.focus, cx);
            this.tell_host(window, cx);
            cx.notify();
        }),
        cx.observe_in(picker, window, |this, picker, window, cx| {
            if this.start_on_load.is_some() && picker.read(cx).command(cx).is_ok() {
                this.start_on_load = None;
                this.start(window, cx);
            }
            cx.notify();
        }),
        cx.subscribe(columns, |this, _, _: &ResizablePanelEvent, cx| {
            this.resized(cx);
        }),
    ]
}

impl Kagan {
    fn columns(
        &mut self,
        main: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let layout = self.layout;
        h_resizable("columns")
            .with_state(&self.columns)
            .when(self.panes.left, |columns| {
                columns.child(
                    resizable_panel()
                        .flex_none()
                        .size(layout.left_width)
                        .size_range(px(layout::LEFT_RANGE.0)..px(layout::LEFT_RANGE.1))
                        .child(self.left_side(window, cx)),
                )
            })
            .child(self.center_column(main, window, cx))
            .when(self.panes.right, |columns| {
                columns.child(
                    resizable_panel()
                        .flex_none()
                        .size(layout.right_width)
                        .size_range(px(right::WIDTH_RANGE.0)..px(right::WIDTH_RANGE.1))
                        .child(right::render(self, window, cx)),
                )
            })
            .into_any_element()
    }

    fn center_column(
        &mut self,
        main: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ResizablePanel {
        let popup = ask_form::popup(self, window, cx);
        resizable_panel()
            .size_range(px(layout::CENTER_MIN)..px(f32::MAX))
            .child(
                v_flex()
                    .id("center")
                    .ui_container()
                    .relative()
                    .size_full()
                    .min_w_0()
                    .when_some(self.note.clone(), |main, note| {
                        main.child(
                            div()
                                .id("note")
                                .ui_container()
                                .aria_label(note.clone())
                                .px_4()
                                .py_2()
                                .type_body()
                                .text_color(cx.theme().danger)
                                .child(note),
                        )
                    })
                    .children(update::notice(cx))
                    .children(
                        self.plugin_prompt_visible(cx)
                            .then(|| Self::plugin_prompt_notice(cx)),
                    )
                    .child(main)
                    .children(overlays::render(self, cx))
                    .children(popup)
                    .when_some(self.toast.clone(), |main, toast| {
                        main.child(Self::render_toast(&toast, cx))
                    }),
            )
    }

    fn center_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        match self.screen.current() {
            screen::Center::Settings => settings::main(self, window, cx).into_any_element(),
            screen::Center::Page(_) => surface::page(self, window, cx),
            screen::Center::Review(_) => {
                let replaced = self.replaced_body(kg_domain::Slot::Review, window, cx);
                self.render_review(replaced, window, cx)
            }
            screen::Center::Archived(_) => archived::list(self, window, cx).into_any_element(),
            screen::Center::Work(Shown::Thread(view)) => {
                self.render_thread(view, cx).into_any_element()
            }
            screen::Center::Work(Shown::Start(project)) => {
                self.render_start(project, cx).into_any_element()
            }
            screen::Center::Work(Shown::Nothing) => Self::render_welcome(cx).into_any_element(),
        }
    }
}

fn toast_button(
    id: &'static str,
    label: SharedString,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    controls::control(
        id,
        Facts::button(label.clone()),
        Button::new(SharedString::from(format!("{id}-control")))
            .small()
            .ghost()
            .label(label)
            .on_click(on_click),
    )
}

/// The key context of the window: its settings context, and what is on screen, so a plugin's
/// command bound to `thread` or `project` fires only while that is shown.
fn window_context(kagan: &Kagan, cx: &App) -> String {
    let mut context = if kagan.in_settings() {
        format!("{CONTEXT} {}", settings::CONTEXT)
    } else {
        CONTEXT.to_owned()
    };
    if kagan.shown_thread(cx).is_some() {
        context.push_str(" thread=true");
    }
    if kagan.shown_project(cx).is_some() {
        context.push_str(" project=true");
    }
    if kagan.screen.drawn(screen::Side::Right) {
        context.push_str(" RightOverlay");
    }
    context
}

impl Kagan {
    fn sidebar_event(&mut self, event: &Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Thread(id) => self.select(id.clone(), window, cx),
            Event::Project(id) => self.show_start(id.clone(), window, cx),
            // Enter on a project starts the picked program there, as the README says. The program list
            // may still be loading, so the start then waits for the picker to have a program.
            Event::Start(id) => {
                self.show_start(id.clone(), window, cx);
                if self.picker.read(cx).command(cx).is_ok() {
                    self.start(window, cx);
                } else {
                    self.start_on_load = Some(id.clone());
                }
            }
            Event::Archive(id) => {
                self.select(id.clone(), window, cx);
                self.archive(&ArchiveThread, window, cx);
            }
            Event::ThreadActions(id) => {
                // Selecting closes the narrow overlay, which holds this row, so keep it open.
                let overlay = self.screen.drawn(Side::Left);
                self.select(id.clone(), window, cx);
                if overlay {
                    self.open_overlay(Side::Left, Activation::Focus, window, cx);
                }
                if let Some(shown) = self.shown_open_thread(cx) {
                    self.actions_menu = None;
                    self.sidebar
                        .update(cx, |sidebar, cx| sidebar.open_row_menu(&shown, window, cx));
                }
            }
            Event::RemoveProject(id) => self.remove_project(id.clone(), cx),
            Event::Reorder { id, before } => {
                self.reorder_project(id.clone(), before.clone(), cx);
            }
            Event::OpenSurface(open) => {
                let op = SurfaceOp::Open {
                    plugin: PluginId(open.plugin.clone()),
                    id: open.id.clone(),
                };
                self.run_surface(op, window, cx);
            }
            Event::ClosePage => self.close_page(&ClosePage, window, cx),
            Event::Threads => self.run_surface(SurfaceOp::Threads, window, cx),
            Event::ManagePlugins => self.manage_plugins(window, cx),
            Event::NewPlugin => self.add_plugin(window, cx),
            Event::Settings => self.toggle_settings(&OpenSettings, window, cx),
            Event::RowPin(id)
            | Event::RowArchive(id)
            | Event::RowRename(id)
            | Event::RowSaveDiagnostics(id)
                if self.shown_thread(cx).as_ref() != Some(id) => {}
            Event::RowPin(_) => self.toggle_pinned(&TogglePinned, window, cx),
            Event::RowArchive(_) => self.archive(&ArchiveThread, window, cx),
            Event::RowRename(_) => self.begin_rename(&RenameThread, window, cx),
            Event::RowSaveDiagnostics(_) => {
                Kagan::save_diagnostics(self, &SaveDiagnostics, window, cx);
            }
            Event::RowMenuClosed => self.close_thread_actions(window, cx),
            Event::Archived => self.show_archived(window, cx),
        }
    }

    fn remove_project(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        let task = Workspace::dispatch(Op::Project(ProjectOp::Remove { id: project }), cx);
        let window = self.window;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| match result {
                    Ok(kg_domain::OpResult::Warnings(warnings)) => {
                        this.refresh(window, cx);
                        this.note =
                            Some(format!("Project removed. {}", warnings.join("; ")).into());
                        cx.notify();
                    }
                    result => this.finish(result, "remove project", window, cx),
                });
            });
        })
        .detach();
    }

    fn reorder_project(
        &mut self,
        id: ProjectId,
        before: Option<ProjectId>,
        cx: &mut Context<Self>,
    ) {
        let task = Workspace::dispatch(Op::Project(ProjectOp::Reorder { id, before }), cx);
        let window = self.window;
        cx.spawn(async move |this, cx| {
            let result = task.await;
            _ = cx.update_window(window, |_, window, cx| {
                _ = this.update(cx, |this, cx| {
                    this.finish(result, "move project", window, cx);
                });
            });
        })
        .detach();
    }

    fn shows(&self, cx: &App) -> Shows {
        Shows {
            thread: self.shown_thread(cx),
            page: self
                .screen
                .page()
                .map(|slot| (slot.plugin.clone(), slot.contribution.clone())),
            tool: self.tool.as_ref().map(|slot| slot.plugin.clone()),
            archived: self.shows_archived(),
            in_settings: self.in_settings(),
            waiting: self
                .replaced
                .chosen(kg_domain::Slot::Threads)
                .map(|_| self.waiting_count(cx))
                .filter(|count| *count > 0),
        }
    }

    pub(crate) fn focus_sidebar(
        &mut self,
        _: &FocusSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tool.is_some() || self.in_settings() {
            self.run_surface(SurfaceOp::Threads, window, cx);
            if self.tool.is_some() {
                return;
            }
            let kagan = cx.entity();
            window.defer(cx, move |window, cx| {
                kagan.update(cx, |this, cx| this.focus_sidebar(&FocusSidebar, window, cx));
            });
            return;
        }
        if !self.panes.left {
            self.open_overlay(Side::Left, Activation::SidebarRow, window, cx);
            return;
        }
        let Some(id) = self.sidebar_target(cx) else {
            let focus = self.sidebar.read(cx).focus().clone();
            window.focus(&focus, cx);
            return;
        };
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal_row(&id, window, cx));
    }

    fn sidebar_target(&self, cx: &App) -> Option<String> {
        let sidebar = self.sidebar.read(cx);
        let candidate = if self.shows_archived() {
            Some("archived".to_owned())
        } else {
            self.shown_thread(cx)
                .map(|id| format!("thread-{id}"))
                .or_else(|| sidebar.chosen().map(|project| format!("project-{project}")))
        };
        sidebar.row_or_first(candidate)
    }
}
