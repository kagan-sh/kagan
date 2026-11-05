use std::collections::BTreeMap;

use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::PluginId;

use super::OpError;

/// One shortcut: the saved name, the GPUI action it binds, the default chord, and the label.
#[derive(Debug, PartialEq, Eq)]
pub struct Shortcut {
    pub name: &'static str,
    pub action: &'static str,
    pub chord: &'static str,
    pub label: &'static str,
}

/// Built-in shortcuts. A saved override uses `name`, so those names stay.
pub const SHORTCUTS: &[Shortcut] = &[
    Shortcut {
        name: "Bigger",
        action: "kagan::Bigger",
        chord: "secondary-=",
        label: "Bigger terminal text",
    },
    Shortcut {
        name: "Smaller",
        action: "kagan::Smaller",
        chord: "secondary--",
        label: "Smaller terminal text",
    },
    Shortcut {
        name: "ActualSize",
        action: "kagan::ActualSize",
        chord: "secondary-0",
        label: "Actual terminal text size",
    },
    Shortcut {
        name: "NewThread",
        action: "kagan::NewThread",
        chord: "secondary-n",
        label: "New thread",
    },
    Shortcut {
        name: "StopThread",
        action: "kagan::StopThread",
        chord: "secondary-.",
        label: "Stop thread",
    },
    Shortcut {
        name: "ArchiveThread",
        action: "kagan::ArchiveThread",
        chord: "secondary-shift-backspace",
        label: "Archive thread",
    },
    Shortcut {
        name: "TogglePinned",
        action: "kagan::TogglePinned",
        chord: "secondary-shift-p",
        label: "Pin or unpin thread",
    },
    Shortcut {
        name: "NextThread",
        action: "kagan::NextThread",
        chord: "secondary-shift-]",
        label: "Next thread",
    },
    Shortcut {
        name: "PreviousThread",
        action: "kagan::PreviousThread",
        chord: "secondary-shift-[",
        label: "Previous thread",
    },
    Shortcut {
        name: "NextThreadNeedingYou",
        action: "kagan::NextThreadNeedingYou",
        chord: "secondary-alt-j",
        label: "Next thread needing you",
    },
    Shortcut {
        name: "Thread1",
        action: "kagan::Thread1",
        chord: "secondary-1",
        label: "Thread 1",
    },
    Shortcut {
        name: "Thread2",
        action: "kagan::Thread2",
        chord: "secondary-2",
        label: "Thread 2",
    },
    Shortcut {
        name: "Thread3",
        action: "kagan::Thread3",
        chord: "secondary-3",
        label: "Thread 3",
    },
    Shortcut {
        name: "Thread4",
        action: "kagan::Thread4",
        chord: "secondary-4",
        label: "Thread 4",
    },
    Shortcut {
        name: "Thread5",
        action: "kagan::Thread5",
        chord: "secondary-5",
        label: "Thread 5",
    },
    Shortcut {
        name: "Thread6",
        action: "kagan::Thread6",
        chord: "secondary-6",
        label: "Thread 6",
    },
    Shortcut {
        name: "Thread7",
        action: "kagan::Thread7",
        chord: "secondary-7",
        label: "Thread 7",
    },
    Shortcut {
        name: "Thread8",
        action: "kagan::Thread8",
        chord: "secondary-8",
        label: "Thread 8",
    },
    Shortcut {
        name: "Thread9",
        action: "kagan::Thread9",
        chord: "secondary-9",
        label: "Thread 9",
    },
    Shortcut {
        name: "OpenSettings",
        action: "kagan::OpenSettings",
        chord: "secondary-,",
        label: "Settings",
    },
    Shortcut {
        name: "UseProjectFolder",
        action: "kagan::UseProjectFolder",
        chord: "secondary-alt-p",
        label: "Project folder",
    },
    Shortcut {
        name: "UseWorktreeFolder",
        action: "kagan::UseWorktreeFolder",
        chord: "secondary-alt-w",
        label: "New worktree",
    },
    Shortcut {
        name: "RetryBranches",
        action: "kagan::RetryBranches",
        chord: "secondary-alt-r",
        label: "Retry branches",
    },
    Shortcut {
        name: "RetryPrograms",
        action: "kagan::RetryPrograms",
        chord: "secondary-alt-l",
        label: "Retry programs",
    },
    Shortcut {
        name: "ToggleCustomCommand",
        action: "kagan::ToggleCustomCommand",
        chord: "secondary-alt-c",
        label: "Custom command",
    },
    Shortcut {
        name: "ThreadActions",
        action: "kagan::ThreadActions",
        chord: "secondary-shift-m",
        label: "Thread actions",
    },
    Shortcut {
        name: "ToggleSidebar",
        action: "kagan::ToggleSidebar",
        chord: "secondary-b",
        label: "Show or hide left sidebar",
    },
    Shortcut {
        name: "ToggleRightSidebar",
        action: "kagan::ToggleRightSidebar",
        chord: "secondary-\\",
        label: "Show or hide right sidebar",
    },
    Shortcut {
        name: "AddProject",
        action: "kagan::AddProject",
        chord: "secondary-shift-o",
        label: "Add project",
    },
    Shortcut {
        name: "SendReview",
        action: "review::SendReview",
        chord: "secondary-enter",
        label: "Send review",
    },
    Shortcut {
        name: "NextHunk",
        action: "review::NextChange",
        chord: "alt-]",
        label: "Next change",
    },
    Shortcut {
        name: "PrevHunk",
        action: "review::PrevChange",
        chord: "alt-[",
        label: "Previous change",
    },
    Shortcut {
        name: "FindInDiff",
        action: "review::FindInDiff",
        chord: "secondary-f",
        label: "Find in diff",
    },
    Shortcut {
        name: "ToggleWrap",
        action: "review::ToggleWrap",
        chord: "alt-z",
        label: "Wrap lines",
    },
    Shortcut {
        name: "NextMatch",
        action: "review::NextMatch",
        chord: "secondary-g",
        label: "Next match",
    },
    Shortcut {
        name: "PrevMatch",
        action: "review::PrevMatch",
        chord: "secondary-alt-g",
        label: "Previous match",
    },
    Shortcut {
        name: "StartAgain",
        action: "kagan::StartAgain",
        chord: "secondary-shift-s",
        label: "Start again",
    },
    Shortcut {
        name: "FocusSidebar",
        action: "kagan::FocusSidebar",
        chord: "secondary-shift-l",
        label: "Focus sidebar",
    },
    Shortcut {
        name: "RestoreThread",
        action: "kagan::RestoreThread",
        chord: "secondary-shift-r",
        label: "Restore thread",
    },
    Shortcut {
        name: "UseBuiltInSidebar",
        action: "kagan::UseBuiltInSidebar",
        chord: "secondary-alt-b",
        label: "Use built-in sidebar",
    },
    Shortcut {
        name: "UseBuiltInReview",
        action: "kagan::UseBuiltInReview",
        chord: "secondary-alt-v",
        label: "Use built-in review",
    },
    Shortcut {
        name: "OpenCommandList",
        action: "kagan::OpenCommandList",
        chord: "secondary-shift-a",
        label: "Command list",
    },
    Shortcut {
        name: "SearchThreads",
        action: "kagan::SearchThreads",
        chord: "secondary-alt-f",
        label: "Search threads…",
    },
    Shortcut {
        name: "NewTerminal",
        action: "kagan::NewTerminal",
        chord: "secondary-t",
        label: "New terminal",
    },
    Shortcut {
        name: "CloseTerminal",
        action: "kagan::CloseTerminal",
        chord: "secondary-shift-w",
        label: "Close terminal",
    },
    Shortcut {
        name: "NextTerminal",
        action: "kagan::NextTerminal",
        chord: "secondary-alt-]",
        label: "Next terminal",
    },
    Shortcut {
        name: "PreviousTerminal",
        action: "kagan::PreviousTerminal",
        chord: "secondary-alt-[",
        label: "Previous terminal",
    },
    Shortcut {
        name: "OpenLocalLink",
        action: "kagan::OpenLocalLink",
        chord: "secondary-alt-o",
        label: "Open local link",
    },
];

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
        key: String,
        /// Drop the saved chord and use the default key
        #[arg(long)]
        #[serde(default)]
        reset: bool,
    },
    /// Leave a plugin's command with no key
    Remove {
        /// `<plugin-id>:<command>`
        action: String,
    },
}
