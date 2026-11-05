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
