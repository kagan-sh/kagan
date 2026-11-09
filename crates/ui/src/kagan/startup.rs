use super::{Changes, Kagan, Layout, Plugins, Shown, TITLE, window::release_window};
use super::{
    activity, archived, attention, browser, changes, details, layout, picker, replace, review,
    right, screen, settings, sidebar, start, surface, tabs,
};
use std::collections::{BTreeMap, BTreeSet};

use gpui_kit::component::input::InputState;
use gpui_kit::component::resizable::{ResizablePanelEvent, ResizableState};
use gpui_kit::{
    AnyElement, AppContext as _, Context, Entity, ScrollHandle, Subscription, Task, Window,
};
use kg_core::{Updater, Workspace, WorkspaceEvent};
use kg_domain::ProjectId;

impl Kagan {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
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
            terminal_font_size: kg_domain::prefs::TERMINAL_FONT_DEFAULT,
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
            right: right::Sidebar::new(cx),
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
            ask_drafts: std::collections::BTreeMap::new(),
            ask_hidden: std::collections::BTreeSet::new(),
            ask_popup: None,
            toast: None,
            toast_epoch: 0,
            activity: activity::Strip::new(cx.focus_handle().tab_stop(true)),
            _update_watch: activity::watch(cx),
            _release: cx.on_release(release_window),
            _poll: changes::poll_open_changes(cx),
            commands: None,
        };
        this.started(window, cx);
        this
    }

    fn started(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.start.branch_focus = Some(cx.focus_handle().tab_stop(true));
        self.start.segments_focus = Some(cx.focus_handle().tab_stop(false));
        self.load_settings(cx);
        self.activity
            .update_state(Updater::global(cx).read(cx).state());
        self.refresh(window, cx);
        self.show_newest(window, cx);
    }

    pub(super) fn show_newest(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let newest = self.listed(cx).next().map(|thread| thread.id.clone());
        match newest {
            Some(newest) => self.select(newest, window, cx),
            None => window.focus(&self.focus, cx),
        }
    }

    pub(super) fn left_side(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let shows = self.shows(cx);
        let tool = surface::tool_region(self, window, cx);
        let threads = self.replaced_body(kg_domain::Slot::Threads, window, cx);
        self.sidebar.update(cx, |sidebar, cx| {
            sidebar::list(sidebar, &shows, threads, tool, window, cx)
        })
    }

    pub(super) fn rail_side(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let shows = self.shows(cx);
        self.sidebar
            .update(cx, |sidebar, cx| sidebar::rail(sidebar, &shows, window, cx))
    }

    pub(super) fn refresh(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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
        self.refresh_plugins(cx);
        self.shown_changed(window, cx);
        cx.notify();
    }
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
                WorkspaceEvent::Event { event, .. } => {
                    this.saw_turn(event, cx);
                    this.refresh(window, cx);
                }
                WorkspaceEvent::Waiting(id) => {
                    this.saw_needs(id, cx);
                    this.waited(id, window, cx);
                }
                WorkspaceEvent::Finished(id) => this.waited(id, window, cx),
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
            this.escape_start(window, cx);
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
