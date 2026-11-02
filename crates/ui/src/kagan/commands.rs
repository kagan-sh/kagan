//! The command list: every window command, with its live key, and thread search.

use std::rc::Rc;

use crate::theme::TypeScale as _;
use gpui_kit::base::actions::Cancel;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::input::Escape;
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, IconName, IndexPath, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    Action, AnyElement, App, AppContext as _, Context, ElementId, Entity, FocusHandle,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString, Styled as _, Task,
    WeakEntity, Window, actions, div, px,
};
use kg_core::Workspace;
use kg_domain::{Found, Op, PluginOp, SHORTCUTS, SettingsOp, WindowOp};
use kg_plugins::{CommandWhen, RunCommand};

use super::{Kagan, Shown};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

mod search;

#[cfg(test)]
pub(crate) use search::found;

actions!(kagan, [OpenCommandList, SearchThreads]);

const TOP: f32 = 48.;
const WIDTH: f32 = 560.;
const HEIGHT: f32 = 420.;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Commands,
    Threads,
}

pub(super) struct CommandList {
    state: Entity<CommandState>,
    mode: Mode,
    back: Option<FocusHandle>,
    found: Vec<Found>,
    failed: Option<SharedString>,
    search: Task<()>,
}

#[derive(Clone)]
enum Run {
    Action(Rc<dyn Action>),
    Op(Rc<Op>),
}

struct Entry {
    id: String,
    label: String,
    run: Run,
    off: Option<&'static str>,
    plugin: Option<String>,
}

pub(super) fn perform(op: WindowOp, cx: &mut App) {
    let mode = if op == WindowOp::SearchThreads {
        Mode::Threads
    } else {
        Mode::Commands
    };
    if super::show(cx).is_err() {
        return;
    }
    let Some((handle, kagan)) = super::front(cx) else {
        return;
    };
    _ = cx.update_window(handle, |_, window, cx| {
        kagan.update(cx, |this, cx| this.open_commands(mode, window, cx));
    });
}

pub(super) fn open_list(_: &OpenCommandList, _: &mut Window, cx: &mut App) {
    Workspace::dispatch(Op::Window(WindowOp::CommandList), cx).detach();
}

pub(super) fn search_threads(_: &SearchThreads, _: &mut Window, cx: &mut App) {
    Workspace::dispatch(Op::Window(WindowOp::SearchThreads), cx).detach();
}

pub(super) fn rail_button(cx: &App) -> impl IntoElement {
    let tip = super::shown::tip("Command list", &OpenCommandList, cx);
    super::shown::control(
        "command-list-open",
        Facts::button("Command list"),
        Button::new("command-list-open-button")
            .small()
            .rounded(px(6.))
            .ghost()
            .icon(IconName::Search)
            .accessibility_label("Command list")
            .tooltip(tip)
            .on_click(|_, _, cx| {
                Workspace::dispatch(Op::Window(WindowOp::CommandList), cx).detach();
            }),
    )
}

impl Kagan {
    fn open_commands(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        let list = match self.commands.take() {
            Some(mut list) => {
                list.mode = mode;
                list.found.clear();
                list.failed = None;
                list.search = Task::ready(());
                list.state
                    .update(cx, |state, cx| state.set_query("", window, cx));
                list
            }
            None => CommandList {
                state: cx.new(|cx| CommandState::new(window, cx)),
                mode,
                back: window.focused(cx),
                found: Vec::new(),
                failed: None,
                search: Task::ready(()),
            },
        };
        list.state.update(cx, |state, cx| state.focus(window, cx));
        self.commands = Some(list);
        cx.notify();
    }

    fn close_commands(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<FocusHandle> {
        let back = self
            .commands
            .take()?
            .back
            .unwrap_or_else(|| self.focus.clone());
        window.focus(&back, cx);
        cx.notify();
        Some(back)
    }

    fn run_entry(&mut self, run: Run, window: &mut Window, cx: &mut Context<Self>) {
        let Some(back) = self.close_commands(window, cx) else {
            return;
        };
        match run {
            Run::Action(action) => window.defer(cx, move |window, cx| {
                back.dispatch_action(action.as_ref(), window, cx);
            }),
            Run::Op(op) => Workspace::dispatch((*op).clone(), cx).detach(),
        }
    }
}

pub(super) fn layer(
    kagan: &Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Option<AnyElement> {
    let list = kagan.commands.as_ref()?;
    let this = cx.entity().downgrade();
    let command = match list.mode {
        Mode::Commands => commands(kagan, list, this, window, cx),
        Mode::Threads => search::threads(list, this),
    };
    Some(
        div()
            .absolute()
            .inset_0()
            .flex()
            .justify_center()
            .items_start()
            .pt(px(TOP))
            .child(
                div()
                    .id("command-list")
                    .ui_container()
                    .occlude()
                    .w(px(WIDTH))
                    .h_auto()
                    .bg(cx.theme().popover)
                    .rounded(cx.theme().radius_lg)
                    .shadow_lg()
                    .capture_action(cx.listener(|this, _: &Cancel, window, cx| {
                        this.close_commands(window, cx);
                        cx.stop_propagation();
                    }))
                    .capture_action(cx.listener(|this, _: &Escape, window, cx| {
                        this.close_commands(window, cx);
                        cx.stop_propagation();
                    }))
                    .on_mouse_down_out(cx.listener(|this, _, window, cx| {
                        this.close_commands(window, cx);
                    }))
                    .child(command.max_h(px(HEIGHT))),
            )
            .into_any_element(),
    )
}

fn commands(
    kagan: &Kagan,
    list: &CommandList,
    this: WeakEntity<Kagan>,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Command {
    let groups = entries(kagan, cx);
    let back = list.back.clone().unwrap_or_else(|| kagan.focus.clone());
    let runs: Rc<Vec<Vec<Run>>> = Rc::new(
        groups
            .iter()
            .map(|(_, entries)| entries.iter().map(|entry| entry.run.clone()).collect())
            .collect(),
    );
    let command =
        groups
            .into_iter()
            .fold(Command::new(&list.state), |command, (title, entries)| {
                let items = entries.into_iter().map(|entry| {
                    let keys = match (&entry.run, entry.off) {
                        (Run::Action(action), None) => window
                            .highest_precedence_binding_for_action_in(action.as_ref(), &back)
                            .map(|binding| {
                                binding
                                    .keystrokes()
                                    .iter()
                                    .map(|key| key.inner().clone())
                                    .collect()
                            })
                            .unwrap_or_default(),
                        _ => Vec::new(),
                    };
                    command_item(entry, keys)
                });
                command.group(CommandGroup::new().label(title).items(items))
            });
    command
        .placeholder("Type a command")
        .on_confirm(move |index: IndexPath, window, cx| {
            let Some(run) = runs
                .get(index.section)
                .and_then(|runs| runs.get(index.row))
                .cloned()
            else {
                return;
            };
            _ = this.update(cx, |this, cx| this.run_entry(run, window, cx));
        })
}

fn command_item(entry: Entry, keys: Vec<gpui_kit::Keystroke>) -> CommandItem {
    let Entry {
        id,
        label,
        off,
        plugin,
        ..
    } = entry;
    let text: String = keys.iter().map(Kbd::format).collect::<Vec<_>>().join(" ");
    CommandItem::new()
        .label(label.clone())
        .keywords(plugin)
        .disabled(off.is_some())
        .child(move |_, cx| {
            let muted = cx.theme().muted_foreground;
            h_flex()
                .id(ElementId::Name(format!("command-{id}").into()))
                .ui_observe_as(Facts {
                    value: Some(text.clone().into()),
                    disabled: off.is_some(),
                    hint: off.map(SharedString::from),
                    ..Facts::button(label.clone())
                })
                .w_full()
                .gap_2()
                .justify_between()
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .text_ellipsis()
                                .when(off.is_some(), |row| row.text_color(muted))
                                .child(label.clone()),
                        )
                        .children(off.map(|off| div().type_small().text_color(muted).child(off))),
                )
                .child(
                    h_flex()
                        .gap_1()
                        .flex_shrink_0()
                        .children(keys.iter().cloned().map(Kbd::new)),
                )
        })
}

fn entries(kagan: &Kagan, cx: &mut Context<Kagan>) -> Vec<(String, Vec<Entry>)> {
    let thread = kagan.shown_thread(cx).is_some();
    let project = kagan.shown_project(cx).is_some();
    let review = kagan.screen.review().is_some();
    let start = matches!(kagan.screen.shown(), Shown::Start(_));
    let off = |group: &str, name: &str| match group {
        "Review" if !review => Some("Open a review first."),
        "New thread" if !start => Some("Open New thread first."),
        _ if !thread && NEEDS_THREAD.contains(&name) => Some("Open a thread first."),
        _ => None,
    };
    let mut groups: Vec<(String, Vec<Entry>)> = GROUPS
        .iter()
        .map(|group| ((*group).to_owned(), Vec::new()))
        .collect();
    let mut add = |group: &str, id: &str, label: &str, run: Run| {
        if let Some((_, entries)) = groups.iter_mut().find(|(title, _)| title == group) {
            entries.push(Entry {
                id: id.to_owned(),
                label: label.to_owned(),
                off: off(group, id),
                run,
                plugin: None,
            });
        }
    };
    for shortcut in SHORTCUTS
        .iter()
        .filter(|shortcut| shortcut.name != "OpenCommandList")
    {
        let Ok(action) = cx.build_action(shortcut.action, None) else {
            continue;
        };
        add(
            group_of(shortcut.name, shortcut.action),
            shortcut.name,
            shortcut.label,
            Run::Action(action.into()),
        );
    }
    for (group, id, label, action) in keyless() {
        add(group, id, label, Run::Action(action));
    }
    add(
        "Plugins",
        "ManagePlugins",
        "Manage plugins",
        Run::Op(Rc::new(Op::Settings(SettingsOp::Plugins))),
    );
    add(
        "Plugins",
        "NewPlugin",
        "New plugin…",
        Run::Op(Rc::new(Op::Plugin(PluginOp::Start))),
    );
    for (plugin, name, commands) in super::settings::plugin_commands(cx) {
        let entries = commands
            .into_iter()
            .map(|command| Entry {
                id: format!("{}-{}", plugin.0, command.id()),
                label: command.title().to_owned(),
                off: match command.when() {
                    CommandWhen::Thread if !thread => Some("Open a thread first."),
                    CommandWhen::Project if !project => Some("Choose a project first."),
                    _ => None,
                },
                run: Run::Action(Rc::new(RunCommand::new(
                    plugin.clone(),
                    command.id().to_owned(),
                ))),
                plugin: Some(name.clone()),
            })
            .collect();
        groups.push((name, entries));
    }
    groups.retain(|(_, entries)| !entries.is_empty());
    groups
}

const GROUPS: [&str; 6] = [
    "Threads",
    "New thread",
    "Review",
    "Window",
    "Settings",
    "Plugins",
];

const NEEDS_THREAD: [&str; 8] = [
    "StopThread",
    "ArchiveThread",
    "TogglePinned",
    "StartAgain",
    "RestoreThread",
    "ThreadActions",
    "RenameThread",
    "SaveDiagnostics",
];

fn group_of(name: &str, action: &str) -> &'static str {
    if action.starts_with("review::") {
        return "Review";
    }
    match name {
        "UseProjectFolder"
        | "UseWorktreeFolder"
        | "RetryBranches"
        | "RetryPrograms"
        | "ToggleCustomCommand" => "New thread",
        "OpenSettings" | "UseBuiltInSidebar" | "UseBuiltInReview" => "Settings",
        "Bigger" | "Smaller" | "ActualSize" | "ToggleSidebar" | "ToggleRightSidebar"
        | "FocusSidebar" | "AddProject" => "Window",
        _ => "Threads",
    }
}

fn keyless() -> [(&'static str, &'static str, &'static str, Rc<dyn Action>); 13] {
    [
        (
            "Threads",
            "RenameThread",
            "Rename thread…",
            Rc::new(super::RenameThread),
        ),
        (
            "Threads",
            "SaveDiagnostics",
            "Save diagnostics",
            Rc::new(super::SaveDiagnostics),
        ),
        (
            "Window",
            "About",
            "About kagan",
            Rc::new(super::menus::About),
        ),
        (
            "Window",
            "Minimize",
            "Minimize",
            Rc::new(super::menus::Minimize),
        ),
        ("Window", "Zoom", "Zoom", Rc::new(super::menus::Zoom)),
        ("Window", "Hide", "Hide kagan", Rc::new(super::menus::Hide)),
        (
            "Window",
            "HideOthers",
            "Hide others",
            Rc::new(super::menus::HideOthers),
        ),
        (
            "Window",
            "ShowAll",
            "Show all",
            Rc::new(super::menus::ShowAll),
        ),
        (
            "Window",
            "Close",
            "Close window",
            Rc::new(super::menus::Close),
        ),
        ("Window", "Quit", "Quit kagan", Rc::new(super::menus::Quit)),
        (
            "Settings",
            "CheckForUpdates",
            "Check for updates",
            Rc::new(super::update::CheckForUpdates),
        ),
        (
            "Settings",
            "NewTheme",
            "New theme…",
            Rc::new(super::NewTheme),
        ),
        (
            "Settings",
            "OpenThemes",
            "Open themes folder",
            Rc::new(super::OpenThemes),
        ),
    ]
}

#[cfg(test)]
pub(crate) fn state(kagan: &Kagan) -> Option<Entity<CommandState>> {
    kagan.commands.as_ref().map(|list| list.state.clone())
}
