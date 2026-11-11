use gpui_kit::{InvalidKeystrokeError, Keystroke, OperatingSystem};
use kg_domain::KeyPlatform;

/// This host's input conventions, as GPUI names them.
#[must_use]
pub fn host_os() -> OperatingSystem {
    match KeyPlatform::host() {
        KeyPlatform::Mac => OperatingSystem::Mac,
        KeyPlatform::Windows => OperatingSystem::Windows,
        KeyPlatform::Linux => OperatingSystem::Linux,
    }
}

/// Parses one keystroke as this host spells it: `secondary` is Cmd on macOS and Ctrl elsewhere.
///
/// # Errors
/// When `key` is not a keystroke.
pub fn parse_key(key: &str) -> Result<Keystroke, InvalidKeystrokeError> {
    Keystroke::parse_with_os(key, host_os())
}

// The named keys GPUI spells, as `parse_key` reads them. A modifier typed alone is a key too
// (GPUI's fallback: `ctrl` alone is `control`, `cmd` alone is `platform`), so those are listed.
// A single character is always a key.
const NAMED_KEYS: [&str; 23] = [
    "backspace",
    "delete",
    "left",
    "right",
    "up",
    "down",
    "pageup",
    "pagedown",
    "insert",
    "home",
    "end",
    "back",
    "forward",
    "escape",
    "enter",
    "tab",
    "space",
    "capslock",
    "shift",
    "control",
    "alt",
    "platform",
    "function",
];

/// Whether `key` names a key that can be pressed: one character, a named key such as `escape`,
/// or a function key from `f1` to `f35`.
#[must_use]
pub fn is_known_key(key: &str) -> bool {
    let mut chars = key.chars();
    let one_character = chars.next().is_some() && chars.next().is_none();
    let function_key = key
        .strip_prefix('f')
        .and_then(|number| number.parse::<u8>().ok())
        .is_some_and(|number| (1..=35).contains(&number));
    one_character || NAMED_KEYS.contains(&key) || function_key
}

#[cfg(test)]
#[path = "keys_test.rs"]
mod tests;
