//! The saved preference keys that more than one crate reads or writes.

pub const CONFIRM_ARCHIVE: &str = "confirm-archive";
pub const AGENT_INTEGRATION: &str = "agent-integration";
pub const NOTIFY: &str = "notify";
pub const PLUGIN_STATUS: &str = "plugin-status";
pub const UPDATE_CHECK: &str = "update-check";
pub const TERMINAL_FONT_SIZE: &str = "terminal-font-size";
pub const REVIEW_VIEWED: &str = "review-viewed";
pub const SETUP_GUIDE: &str = "setup-guide";

pub const TERMINAL_FONT_MIN: u8 = 9;
pub const TERMINAL_FONT_MAX: u8 = 24;
pub const TERMINAL_FONT_DEFAULT: u8 = 12;

pub const LEFT_SIDEBAR: &str = "left-sidebar";
pub const LEFT_WIDTH: &str = "left-width";
pub const RIGHT_SIDEBAR: &str = "right-sidebar";
pub const RIGHT_WIDTH: &str = "right-width";
pub const REVIEW_WRAP: &str = "review-wrap";
