use clap::Subcommand;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case")]
pub enum UpdateOp {
    /// Look for a newer kagan on the GitHub releases page now
    Check,
    /// The running version, how this copy was installed, and what the last check found. Only a
    /// person installs an update, from the window
    Status,
}

/// How this copy of kagan was installed, which decides how it updates.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InstallKind {
    /// The macOS app from the dmg: kagan replaces it and restarts.
    App,
    /// The macOS app from the Homebrew cask: brew updates it.
    Homebrew,
    /// The Windows installer: kagan runs the new installer after it quits.
    Installer,
    /// A Linux `AppImage`: kagan replaces it and restarts.
    AppImage,
    /// A .deb package or a source build: the person updates it.
    #[default]
    Other,
}

impl InstallKind {
    #[must_use]
    pub fn replaces_itself(self) -> bool {
        matches!(self, Self::App | Self::Installer | Self::AppImage)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum UpdateState {
    Unchecked,
    Checking,
    Current,
    Available {
        version: String,
        notes: String,
    },
    Downloading {
        version: String,
    },
    /// Downloaded and its signature checked. With `at_quit` it installs when kagan quits.
    Ready {
        version: String,
        notes: String,
        at_quit: bool,
    },
    Failed {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub version: String,
    pub install: InstallKind,
    pub automatic: bool,
    pub state: UpdateState,
}
