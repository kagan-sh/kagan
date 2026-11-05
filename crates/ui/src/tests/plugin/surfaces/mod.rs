mod focus;
mod install;
mod layout;
mod menus;
mod narrow;
mod open_close;
mod overlays;
mod pages;
mod panels;
mod prompt;
mod rail;
mod webview;

pub(in crate::tests) use install::{
    body, box_of, close_surface, height_of, install_broken, install_many, install_pair, item,
    manifest, open_surface, run, surfaces, wide, width_of, write_plugin,
};
pub(in crate::tests) use layout::{
    BLANK, FILL, focus_page_input, install_blank, install_fill, replied, saved, show_thread,
    tab_to, terminal_focused, titles,
};
pub(in crate::tests) use narrow::narrow;
pub(in crate::tests) use open_close::{assert_replaced, emit_open, screen, set_mode};
pub(in crate::tests) use pages::{origin_x, origin_y};
pub(in crate::tests) use panels::set_plugin;
pub(in crate::tests) use prompt::{plugin_agent, use_environment, wait_for_plugin};
pub(in crate::tests) use rail::{THROWING_OVERLAY, TINT, TINT_OVERLAY};
