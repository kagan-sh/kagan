//! The keys a plugin may bind. The window's own keys and `kg keymap` share these checks, so a
//! plugin never saves a key the window would refuse.

use std::rc::Rc;

use gpui_kit::{App, KeyBinding, KeyBindingContextPredicate};

use crate::commands::RunCommand;

/// The context the window's keys are bound in. They apply while the window has focus.
pub const WINDOW_CONTEXT: &str = "Kagan";

/// A key sequence that is taken, and what takes it, by its `kg keymap` name.
pub type Taken = (Vec<String>, String);

/// The window's own keys: every binding that is not a plugin command and can fire in the window.
/// A built-in shortcut is named as `kg keymap` names it, such as `NewThread`.
#[must_use]
pub fn window_keys(cx: &App) -> Vec<Taken> {
    cx.key_bindings()
        .borrow()
        .bindings()
        .filter(|binding| !binding.action().as_any().is::<RunCommand>())
        .filter(|binding| in_window(binding.predicate()))
        .map(|binding| {
            let action = binding.action().name();
            let name = kg_domain::SHORTCUTS
                .iter()
                .find(|shortcut| shortcut.action == action)
                .map_or(action, |shortcut| shortcut.name);
            (sequence(binding), name.to_owned())
        })
        .collect()
}

/// What the first taken sequence that `keys` overlaps is taken by, if any.
#[must_use]
pub fn conflict(taken: &[Taken], keys: &[String]) -> Option<String> {
    taken
        .iter()
        .find(|(taken, _)| overlaps(taken, keys))
        .map(|(_, by)| by.clone())
}

/// Whether two key sequences can both press: one starts with the other, so pressing the shorter one
/// would take the first key of the longer one.
#[must_use]
pub fn overlaps(left: &[String], right: &[String]) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

/// The keys of a key text such as `secondary-n x`, one keystroke each, as text.
#[must_use]
pub fn text_sequence(key: &str) -> Vec<String> {
    key.split_whitespace().map(str::to_owned).collect()
}

/// The keys a binding presses, in order, as text.
#[must_use]
pub fn sequence(binding: &KeyBinding) -> Vec<String> {
    binding
        .keystrokes()
        .iter()
        .map(|key| key.unparse(kg_core::host_os()))
        .collect()
}

fn in_window(predicate: Option<Rc<KeyBindingContextPredicate>>) -> bool {
    predicate.is_none_or(|predicate| predicate.to_string().contains(WINDOW_CONTEXT))
}
