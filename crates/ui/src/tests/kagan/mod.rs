use kg_domain::{KeyPlatform, Keymap};

use crate::kagan::chord_for;

/// The chord the window answers to for an action on this machine, as the keyboard sends it.
pub(super) fn chord(action: &str) -> String {
    chord_for(action, &Keymap::default(), KeyPlatform::host())
}

mod agent_rules;
mod approvals;
mod browser_tab;
mod commands;
mod ended;
mod keys;
mod list_options;
mod marks;
mod paint;
mod panels;
mod plugin_browser;
mod plugins;
mod remove;
mod review;
mod review_comments;
mod review_wrap;
mod setup;
mod setup_guide;
mod sidebar;
mod skills;
mod start;
mod support;
mod surfaces;
mod terminal;
mod threads;
mod usage;
mod worktree;

pub(in crate::tests) use approvals::approvals_waiting;
pub(in crate::tests) use commands::{NAVIGATOR, run_navigator};
#[cfg(unix)]
pub(in crate::tests) use keys::notice_of;
pub(in crate::tests) use marks::{install_navigator, plugin_log, wait_for_mark};
#[cfg(unix)]
pub(in crate::tests) use paint::host_dimensions;
pub(in crate::tests) use paint::{
    focused_of, highlighted_at, label, painted, point_at, present, quad_at_row, role_of, row_top,
    selected_of, shows, value_of,
};
#[cfg(unix)]
pub(in crate::tests) use plugins::host;
pub(in crate::tests) use plugins::{
    PAGE, PAGE_FRAME, add_project, committed_repo, init_repo, install_page, open_lines,
    open_with_plugins, open_with_plugins_then, repo_thread, run_hidden_shell,
};
pub(in crate::tests) use remove::right_click;
#[cfg(unix)]
pub(in crate::tests) use remove::{only_worktree, worktree_dirs};
pub(in crate::tests) use review::two_changes;
pub(in crate::tests) use setup::{commit_setup, disabled};
pub(in crate::tests) use start::press_key;
#[cfg(unix)]
pub(in crate::tests) use support::app_then;
pub(in crate::tests) use support::setup_with;
pub(in crate::tests) use support::{
    click, click_within, count_repaints, git, grid_text, host_grid, host_then, input, pending,
    run_child, run_program, screen_text, scroll_form_to_end, set_preference, setup, thread_record,
    thread_status, wait_for_output, wait_for_screen, wait_until,
};
pub(crate) use support::{open, open_empty, run_shell};
pub(in crate::tests) use surfaces::{
    click_through_driver, install_overlay, install_twin, observed_label, open_surface,
    overlay_source, run_op,
};
#[cfg(unix)]
pub(in crate::tests) use terminal::{cell_point, selection, shown_row};
pub(in crate::tests) use terminal::{
    drag_to, hover_to, line, press, release, send, shell, split_command,
};
#[cfg(unix)]
pub(in crate::tests) use worktree::threads_in;
pub(in crate::tests) use worktree::{environment_of, head_commit, set_program, shown};
