use std::collections::BTreeMap;

use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PluginId;

use super::OpError;

mod shortcuts;

pub use shortcuts::{
    CommandGroup, SHORTCUTS, Shortcut, ShortcutGroup, ShortcutScope, ShortcutWhen,
};

/// A shortcut as `kg keymap` names it: a built-in by its name, or a plugin's command as
/// `<plugin-id>:<command>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeymapAction {
    BuiltIn(&'static Shortcut),
    Plugin { plugin: PluginId, command: String },
}

impl KeymapAction {
    /// # Errors
    ///
    /// Returns `Invalid` for a name that is neither a built-in shortcut nor `<plugin-id>:<command>`.
    pub fn parse(action: &str) -> Result<Self, OpError> {
        if let Some((plugin, command)) = action.split_once(':') {
            if plugin.is_empty() || command.is_empty() {
                return Err(OpError::Invalid(format!(
                    "{action} is not <plugin-id>:<command>"
                )));
            }
            return Ok(Self::Plugin {
                plugin: PluginId(plugin.to_owned()),
                command: command.to_owned(),
            });
        }
        SHORTCUTS
            .iter()
            .find(|shortcut| shortcut.name == action)
            .map(Self::BuiltIn)
            .ok_or_else(|| OpError::Invalid(format!("unknown shortcut {action}")))
    }
}

impl std::fmt::Display for KeymapAction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BuiltIn(shortcut) => f.write_str(shortcut.name),
            Self::Plugin { plugin, command } => write!(f, "{plugin}:{command}"),
        }
    }
}

/// The chord an action answers to when the person has not saved one. Off macOS, `secondary` is
/// Ctrl+Shift, because a bare Ctrl chord belongs to the program.
#[must_use]
pub fn default_chord(name: &str, platform: KeyPlatform) -> Option<String> {
    SHORTCUTS
        .iter()
        .find(|shortcut| shortcut.name == name)
        .map(|shortcut| expand_secondary(shortcut.chord, platform))
}

/// `secondary` is the platform key on macOS and Ctrl+Shift elsewhere. A following `shift-` is already in Ctrl+Shift.
#[must_use]
pub fn expand_secondary(chord: &str, platform: KeyPlatform) -> String {
    if platform == KeyPlatform::Mac {
        return chord.to_owned();
    }
    match chord.strip_prefix("secondary-") {
        Some(rest) => format!("ctrl-shift-{}", rest.trim_start_matches("shift-")),
        None => chord.to_owned(),
    }
}

/// Which machine the window is on. `secondary` stays the command key on macOS and becomes Ctrl+Shift elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum KeyPlatform {
    Mac,
    Windows,
    Linux,
}

impl KeyPlatform {
    #[must_use]
    pub fn host() -> Self {
        if cfg!(target_os = "macos") {
            Self::Mac
        } else if cfg!(target_os = "windows") {
            Self::Windows
        } else {
            Self::Linux
        }
    }
}

/// Saved shortcut overrides. One data directory is one machine, so a chord is not scoped to a platform.
/// An older map that named `mac`, `windows`, or `linux` keeps only its `shared` part.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Keymap {
    #[serde(default)]
    pub shared: BTreeMap<String, String>,
}

impl Keymap {
    /// Reads `shared`, or an older flat map of action to chord.
    ///
    /// # Errors
    ///
    /// Returns an error when `value` is JSON this map cannot read.
    pub fn from_setting(value: Option<Value>) -> Result<Self, serde_json::Error> {
        let Some(value) = value else {
            return Ok(Self::default());
        };
        let scoped = value.as_object().is_some_and(|map| {
            map.keys()
                .any(|key| matches!(key.as_str(), "shared" | "mac" | "windows" | "linux"))
        });
        if scoped {
            Ok(Self {
                shared: serde_json::from_value(
                    value
                        .get("shared")
                        .cloned()
                        .unwrap_or_else(|| Value::Object(serde_json::Map::new())),
                )?,
            })
        } else {
            Ok(Self {
                shared: serde_json::from_value(value)?,
            })
        }
    }

    #[must_use]
    pub fn binding(&self, action: &str) -> Option<&str> {
        self.shared.get(action).map(String::as_str)
    }

    pub fn set(&mut self, action: String, key: String) {
        self.shared.insert(action, key);
    }

    /// Leaves a shortcut with no key. An empty chord is the saved mark for that.
    pub fn unbind(&mut self, action: String) {
        self.shared.insert(action, String::new());
    }

    pub fn reset(&mut self, action: &str) {
        self.shared.remove(action);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum KeymapOp {
    /// Saved shortcut overrides, and the key of each enabled plugin's commands
    Get,
    /// Rebind a shortcut, or reset it
    Set {
        /// Action name, such as `ToggleSidebar`, or `<plugin-id>:<command>` for a plugin's command
        action: String,
        /// Chord to record. Empty with `--reset`.
        #[arg(default_value = "")]
        key: String,
        /// Drop the saved chord and use the default key
        #[arg(long)]
        #[serde(default)]
        reset: bool,
    },
    /// Leave a shortcut with no key, until it is set or reset
    Remove {
        /// `<plugin-id>:<command>`
        action: String,
    },
}
