use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::{
    Action, App, Global, KeyBinding, KeyBindingContextPredicate, NoAction, Subscription,
};
use kg_core::Workspace;
use kg_domain::{KeyPlatform, Keymap, SHORTCUTS, ShortcutScope};

use super::{
    Bigger, CONTEXT, CloseCustomCommand, DismissLeftOverlay, DismissRightOverlay, settings, sidebar,
};
use crate::terminal;

/// The keys the terminal does not take: the window's focus keys stay out of the program. Copy,
/// paste and page scroll are the window's; the program never sees them.
fn terminal_bindings() -> Vec<KeyBinding> {
    let (copy, paste) = if cfg!(target_os = "macos") {
        ("cmd-c", "cmd-v")
    } else {
        ("ctrl-shift-c", "ctrl-shift-v")
    };
    vec![
        KeyBinding::new("tab", NoAction, Some(terminal::CONTEXT)),
        KeyBinding::new("shift-tab", NoAction, Some(terminal::CONTEXT)),
        KeyBinding::new("ctrl-c", NoAction, Some(terminal::CONTEXT)),
        KeyBinding::new(copy, terminal::CopySelection, Some(terminal::CONTEXT)),
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
    // A saved chord that does not parse would panic when bound, so it counts as no override.
    if let Some(saved) = keymap
        .binding(action)
        .filter(|saved| kg_core::parse_key(saved).is_ok())
    {
        return saved.to_owned();
    }
    kg_domain::default_chord(action, platform).unwrap_or_default()
}

/// The window's shortcuts, each on the chord `chord_for` gives it.
fn chord_bindings(keymap: &Keymap, cx: &App) -> Vec<KeyBinding> {
    SHORTCUTS
        .iter()
        .flat_map(|shortcut| {
            let contexts: &[&str] = match shortcut.scope {
                ShortcutScope::Window => &[CONTEXT],
                ShortcutScope::Review => &[super::review::CONTEXT],
                ShortcutScope::ReviewSend => &["ReviewNav", "Review > Input"],
            };
            contexts.iter().filter_map(move |context| {
                let result = (|| {
                    let action = cx
                        .build_action(shortcut.action, None)
                        .map_err(|error| error.to_string())?;
                    let context = KeyBindingContextPredicate::parse(context)
                        .map_err(|error| error.to_string())?;
                    KeyBinding::load(
                        &chord_for(shortcut.name, keymap, KeyPlatform::host()),
                        action,
                        Some(context.into()),
                        false,
                        None,
                        cx,
                    )
                    .map_err(|error| error.to_string())
                })();
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

/// Keeps the saved watch alive for the app's lifetime.
struct ChordWatch {
    _watch: Subscription,
}
impl Global for ChordWatch {}

/// Installs the window's keys, then rebinds the window's chords whenever the workspace changes, so
/// a saved shortcut takes effect at once. Call after `Workspace::open`.
pub(crate) fn install_keys(cx: &mut App) {
    super::menus::bind(cx);
    super::update::bind(cx);
    cx.bind_keys(terminal_bindings());
    cx.bind_keys(tree_bindings());
    cx.bind_keys(sidebar::bindings());
    cx.bind_keys(super::archived::bindings());
    cx.bind_keys(settings::bindings());
    cx.bind_keys(super::changes::bindings());
    cx.bind_keys(super::plugin_actions::bindings());
    cx.bind_keys(super::right::bindings());
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
            super::start::CloseBranchMenu,
            Some(super::start::BRANCH_CONTEXT),
        ),
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
            Some("ReviewNav"),
        ),
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "cmd-c"
            } else {
                "ctrl-shift-c"
            },
            gpui_kit::component::input::Copy,
            Some("ReviewNav"),
        ),
        KeyBinding::new("j", super::review::NextFile, Some("ReviewNav")),
        KeyBinding::new("k", super::review::PrevFile, Some("ReviewNav")),
        KeyBinding::new("n", super::review::NextChange, Some("ReviewNav")),
        KeyBinding::new("p", super::review::PrevChange, Some("ReviewNav")),
        KeyBinding::new("v", super::review::ToggleViewed, Some("ReviewNav")),
        KeyBinding::new(
            "escape",
            super::review::CloseReview,
            Some(super::review::CONTEXT),
        ),
        KeyBinding::new("up", SelectUp, Some(super::picker::CONTEXT)),
        KeyBinding::new("down", SelectDown, Some(super::picker::CONTEXT)),
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

/// A binding for a key known only at run time, read the way this host spells it. GPUI's
/// `KeyBinding::new` panics on a key it cannot parse, such as `secondary-` without a host.
pub(crate) fn bind(
    key: &str,
    action: impl Action,
    context: &str,
    cx: &App,
) -> Result<KeyBinding, String> {
    let context = KeyBindingContextPredicate::parse(context).map_err(|error| error.to_string())?;
    KeyBinding::load(key, Box::new(action), Some(context.into()), false, None, cx)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::chord_bindings;
    use gpui_kit::TestAppContext;
    use kg_domain::{Keymap, SHORTCUTS};

    #[gpui_kit::test]
    fn every_shortcut_builds_an_action(cx: &mut TestAppContext) {
        cx.update(|cx| {
            assert_eq!(
                chord_bindings(&Keymap::default(), cx).len(),
                SHORTCUTS.len() + 1
            );
            for shortcut in SHORTCUTS {
                assert!(
                    cx.build_action(shortcut.action, None).is_ok(),
                    "{}",
                    shortcut.action
                );
            }
        });
    }
}
