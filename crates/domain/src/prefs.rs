//! The saved preference keys that more than one crate reads or writes.

pub const CONFIRM_ARCHIVE: &str = "confirm-archive";
pub const AGENT_INTEGRATION: &str = "agent-integration";
pub const NOTIFY: &str = "notify";
pub const PLUGIN_STATUS: &str = "plugin-status";
pub const UPDATE_CHECK: &str = "update-check";
pub const USAGE_LIMITS: &str = "usage-limits";
pub const BRANCH_PREFIX: &str = "branch-prefix";
pub const TERMINAL_COPY_ON_SELECT: &str = "terminal-copy-on-select";
pub const TERMINAL_LINKS_BROWSER: &str = "terminal-links-browser";
pub const FONT_UI: &str = "font-ui";
pub const FONT_TERMINAL: &str = "font-terminal";
pub const FONT_NAME_MAX: usize = 80;
pub const DENSITY: &str = "density";
pub const DENSITIES: [&str; 3] = ["compact", "default", "roomy"];
pub const BRANCH_PREFIX_MAX: usize = 40;
pub const TERMINAL_FONT_SIZE: &str = "terminal-font-size";
pub const REVIEW_VIEWED: &str = "review-viewed";
pub const SETUP_GUIDE: &str = "setup-guide";
pub const SIDEBAR_ORDER: &str = "sidebar-order";
pub const SIDEBAR_ORDER_CURRENT: &str = "current";
pub const SIDEBAR_ORDER_ATTENTION: &str = "attention";
pub const SIDEBAR_ORDERS: [&str; 2] = [SIDEBAR_ORDER_CURRENT, SIDEBAR_ORDER_ATTENTION];
pub const SIDEBAR_HIDE_EMPTY_PROJECTS: &str = "sidebar-hide-empty-projects";
pub const SIDEBAR_UNREAD_FIRST: &str = "sidebar-unread-first";
pub const RAIL_HIDDEN: &str = "rail-hidden";

pub const TERMINAL_FONT_MIN: u8 = 9;
pub const TERMINAL_FONT_MAX: u8 = 24;
pub const TERMINAL_FONT_DEFAULT: u8 = 12;

pub const LEFT_SIDEBAR: &str = "left-sidebar";
pub const LEFT_WIDTH: &str = "left-width";
pub const RIGHT_SIDEBAR: &str = "right-sidebar";
pub const RIGHT_WIDTH: &str = "right-width";
pub const REVIEW_WRAP: &str = "review-wrap";
/// The widths, in pixels, a window's sidebars may be saved at. The right maximum is only a sanity
/// bound: the center's minimum width is what stops a drag (decision 272).
pub const LEFT_WIDTH_RANGE: (f32, f32) = (200., 420.);
pub const RIGHT_WIDTH_RANGE: (f32, f32) = (240., 3840.);

/// The rule a branch prefix follows. Settings and the `kg` error both say it.
pub const BRANCH_PREFIX_RULE: &str = "It must end in / or -, such as bb/ or feat-.";

/// Whether `text` can start the branch names kagan makes: empty, or a git branch fragment such as `bb/`.
#[must_use]
pub fn valid_branch_prefix(text: &str) -> bool {
    text.is_empty()
        || (text.len() <= BRANCH_PREFIX_MAX
            && (text.ends_with('/') || text.ends_with('-'))
            && !text.starts_with("refs/")
            && crate::valid_branch(&format!("{text}x")))
}

/// Whether `text` can name a font family: empty for the theme's own font, or a family name of up to 80 bytes.
#[must_use]
pub fn valid_font_name(text: &str) -> bool {
    text.is_empty() || (text.len() <= FONT_NAME_MAX && !text.chars().any(crate::unsafe_char))
}
