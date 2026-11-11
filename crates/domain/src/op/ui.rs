use std::path::PathBuf;

use clap::{Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};

pub const UI_UNAVAILABLE: &str = "the ui driver needs a kagan window";

pub const UI_PERSON_ONLY: &str = "person-only approvals cannot be answered with kg ui";

pub const UI_SCREENSHOT_UNAVAILABLE: &str = "ui screenshot is not available on this platform";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiBounds {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiElement {
    pub id: String,
    pub path: Vec<String>,
    pub role: Option<String>,
    pub label: Option<String>,
    pub value: Option<String>,
    pub bounds: UiBounds,
    pub focused: Option<bool>,
    pub disabled: Option<bool>,
    pub selected: Option<bool>,
    pub visible: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiSnapshot {
    pub window: u32,
    pub elements: Vec<UiElement>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Subcommand)]
#[serde(rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum UiOp {
    /// Observed elements with stable ids, roles, bounds, and focus state
    Snapshot {
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Click the center of an element. `--within` picks the one inside that element, when several match
    Click {
        element_id: String,
        #[arg(long)]
        within: Option<String>,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Send text to the focused control
    Type {
        text: String,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Send a keystroke such as escape or cmd-,
    Key {
        keystroke: String,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Scroll an element by dy pixels: a negative dy scrolls down, a positive dy scrolls up
    Scroll {
        element_id: String,
        #[arg(allow_negative_numbers = true)]
        dy: f32,
        #[arg(long)]
        within: Option<String>,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Press an element, drag it onto another, and release. `--at` picks the target's top quarter,
    /// middle or bottom quarter. `--hold` keeps the button down: finish with `kg ui drop`, or cancel
    /// with `kg ui key escape`
    Drag {
        from: String,
        to: String,
        #[arg(long, value_enum, default_value_t)]
        at: DropSide,
        /// Pick both elements inside the element with this id
        #[arg(long)]
        within: Option<String>,
        #[arg(long)]
        hold: bool,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Release a drag that `kg ui drag --hold` left held, where the pointer is
    Drop {
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
    /// Save a PNG without focusing the window
    Screenshot {
        path: PathBuf,
        #[arg(long, default_value_t = 0)]
        window: u32,
    },
}

/// Where on a drop target `kg ui drag` lets go, as a plugin's drop handler reports it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum DropSide {
    Top,
    #[default]
    Middle,
    Bottom,
}
