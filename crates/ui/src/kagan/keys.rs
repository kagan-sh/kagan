use gpui_kit::{
    Action, App, Global, KeyBinding, KeyBindingContextPredicate, NoAction, Subscription,
};
use kg_core::Workspace;
use kg_domain::{KeyPlatform, Keymap, SHORTCUTS, ShortcutScope};

use super::{
    Bigger, CONTEXT, CloseCustomCommand, DismissLeftOverlay, DismissRightOverlay, settings, sidebar,
};
use crate::terminal;

const COPY: &str = if cfg!(target_os = "macos") {
    "cmd-c"
} else {
    "ctrl-shift-c"
};
pub(crate) const NAV_CONTEXT: &str = "ReviewNav";

fn contextual_bindings() -> Vec<KeyBinding> {
    terminal_bindings()
        .into_iter()
        .chain(tree_bindings())
        .chain(super::commands::saved::bindings())
        .collect()
}

fn terminal_bindings() -> Vec<KeyBinding> {
    let paste = if cfg!(target_os = "macos") {
        "cmd-v"
    } else {
        "ctrl-shift-v"
    };
    vec![
        KeyBinding::new("tab", NoAction, Some(terminal::CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(terminal::CONTEXT)),
        KeyBinding::new("ctrl-c", NoAction, Some(terminal::CONTEXT)),
        KeyBinding::new(COPY, terminal::CopySelection, Some(terminal::CONTEXT)),
        KeyBinding::new(paste, terminal::Paste, Some(terminal::CONTEXT)),
        KeyBinding::new(
            "shift-pageup",
            terminal::ScrollPageUp,
            Some(terminal::CONTEXT),
        ),
        KeyBinding::new(
            "shift-pagedown",
            terminal::ScrollPageDown,
            Some(terminal::CONTEXT),
        ),
    ]
}

pub(crate) fn chord_for(action: &str, keymap: &Keymap, platform: KeyPlatform) -> String {
    // An empty saved chord is a shortcut the person took the key off. A saved chord that does not
    // parse would panic when bound, so it counts as no override.
    if let Some(saved) = keymap
        .binding(action)
        .filter(|saved| saved.is_empty() || kg_core::parse_key(saved).is_ok())
    {
        return saved.to_owned();
    }
    kg_domain::default_chord(action, platform).unwrap_or_default()
}

fn chord_bindings(keymap: &Keymap, cx: &App) -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .flat_map(|shortcut| {
            let chord = chord_for(shortcut.name, keymap, KeyPlatform::host());
            let contexts: &[&str] = match shortcut.scope {
                ShortcutScope::Window => &[CONTEXT],
                ShortcutScope::Review => &[super::review::CONTEXT],
                ShortcutScope::ReviewSend => &[NAV_CONTEXT, "Review > Input"],
            };
            contexts.iter().filter_map(move |context| {
                if chord.is_empty() {
                    return None;
                }
                let result = cx
                    .build_action(shortcut.action, None)
                    .map_err(|error| error.to_string())
                    .and_then(|action| bind(&chord, action, context, cx));
                match result {
                    Ok(binding) => Some(binding),
                    Err(error) => {
                        eprintln!("kg: could not bind shortcut {}: {error}", shortcut.action);
                        None
                    }
                }
            })
        })
        .collect()
}

pub(crate) fn is_chord(action: &dyn Action) -> bool {
    let name = action.name();
    SHORTCUTS.iter().any(|shortcut| shortcut.action == name)
}

fn tree_bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new(
            "up",
            super::review::tree::Up,
            Some(super::review::tree::CONTEXT),
        ),
        KeyBinding::new(
            "down",
            super::review::tree::Down,
            Some(super::review::tree::CONTEXT),
        ),
        KeyBinding::new(
            "left",
            super::review::tree::Left,
            Some(super::review::tree::CONTEXT),
        ),
        KeyBinding::new(
            "right",
            super::review::tree::Right,
            Some(super::review::tree::CONTEXT),
        ),
        KeyBinding::new(
            "home",
            super::review::tree::Home,
            Some(super::review::tree::CONTEXT),
        ),
        KeyBinding::new(
            "end",
            super::review::tree::End,
            Some(super::review::tree::CONTEXT),
        ),
        KeyBinding::new(
            "enter",
            super::review::tree::Open,
            Some(super::review::tree::CONTEXT),
        ),
    ]
}

// Keeps the saved watch alive for the app's lifetime.
struct ChordWatch {
    _watch: Subscription,
}
impl Global for ChordWatch {}

pub(crate) fn install_keys(cx: &mut App) {
    super::menus::bind(cx);
    super::update::bind(cx);
    cx.bind_keys(contextual_bindings());
    cx.bind_keys(sidebar::bindings());
    cx.bind_keys(super::skills::bindings());
    cx.bind_keys(super::archived::bindings());
    cx.bind_keys(settings::bindings());
    cx.bind_keys(settings::project::bindings());
    cx.bind_keys(super::changes::bindings());
    cx.bind_keys(super::plugin_actions::bindings());
    cx.bind_keys(super::right::bindings());
    cx.bind_keys(super::picker::bindings());
    cx.bind_keys(super::surface::bindings());
    cx.bind_keys([
        KeyBinding::new("cmd-+", Bigger, Some(CONTEXT)),
        KeyBinding::new(
            "escape",
            DismissRightOverlay,
            Some(super::layout::RIGHT_OVERLAY),
        ),
        KeyBinding::new(
            "escape",
            DismissLeftOverlay,
            Some(super::layout::LEFT_OVERLAY),
        ),
        KeyBinding::new("escape", CloseCustomCommand, Some(super::picker::CONTEXT)),
        KeyBinding::new(
            "escape",
            super::CloseActivityLog,
            Some(super::activity::CONTEXT),
        ),
        KeyBinding::new(
            "escape",
            super::start::CloseBranchMenu,
            Some(super::start::BRANCH_CONTEXT),
        ),
        KeyBinding::new(
            "escape",
            super::start::LeaveStart,
            Some(super::start::START_CONTEXT),
        ),
        KeyBinding::new(
            "enter",
            super::start::StartThread,
            Some(super::start::START_CONTEXT),
        ),
        KeyBinding::new("enter", NoAction, Some(super::start::CONTROLS_CONTEXT)),
        KeyBinding::new(
            "escape",
            super::ask_form::DismissAskPopup,
            Some(super::ask_form::CONTEXT),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-a"
            } else {
                "ctrl-a"
            },
            gpui_kit::component::input::SelectAll,
            Some(NAV_CONTEXT),
        ),
        KeyBinding::new(COPY, gpui_kit::component::input::Copy, Some(NAV_CONTEXT)),
        KeyBinding::new("j", super::review::NextFile, Some(NAV_CONTEXT)),
        KeyBinding::new("k", super::review::PrevFile, Some(NAV_CONTEXT)),
        KeyBinding::new("n", super::review::NextChange, Some(NAV_CONTEXT)),
        KeyBinding::new("p", super::review::PrevChange, Some(NAV_CONTEXT)),
        KeyBinding::new("v", super::review::ToggleViewed, Some(NAV_CONTEXT)),
        KeyBinding::new(
            "escape",
            super::review::CloseReview,
            Some(super::review::CONTEXT),
        ),
    ]);
    let workspace = Workspace::global(cx);
    let mut applied = workspace.read(cx).keymap().clone();
    let bindings = chord_bindings(&applied, cx);
    cx.bind_keys(bindings);
    super::menus::refresh(cx);
    let watch = cx.observe(&workspace, move |workspace, cx| {
        let keymap = workspace.read(cx).keymap().clone();
        if keymap == applied {
            return;
        }
        let mut kept: Vec<KeyBinding> = cx
            .key_bindings()
            .borrow()
            .bindings()
            .filter(|binding| !is_chord(binding.action()))
            .cloned()
            .collect();
        kept.extend(chord_bindings(&keymap, cx));
        applied = keymap;
        cx.clear_key_bindings();
        cx.bind_keys(kept);
        // Plugin keys are bound against the window's keys, so they follow a changed shortcut.
        crate::plugins::rebind(cx);
        super::menus::refresh(cx);
    });
    cx.set_global(ChordWatch { _watch: watch });
}

// A binding for a key known only at run time, read the way this host spells it. GPUI's
// `KeyBinding::new` panics on a key it cannot parse, such as `secondary-` without a host.
pub(crate) fn bind(
    key: &str,
    action: Box<dyn Action>,
    context: &str,
    cx: &App,
) -> Result<KeyBinding, String> {
    let context = KeyBindingContextPredicate::parse(context).map_err(|error| error.to_string())?;
    KeyBinding::load(key, action, Some(context.into()), false, None, cx)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests;
