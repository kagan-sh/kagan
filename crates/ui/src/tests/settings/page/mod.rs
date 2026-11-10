mod general;
mod install;
mod installed;
mod keyboard;
mod keys;
mod layout;
mod mode;
mod plugins;
mod saves;
mod sections;
mod settings;
mod sidebars;
mod theme;
mod values;

pub(in crate::tests) use general::{THEMES, install_theme_plugin, painted_bg, try_op};
pub(in crate::tests) use install::{keyboard_with_navigator, keycap, nav, nav_note, plugin_keys};
pub(in crate::tests) use installed::{
    TIDE, approval_count, approve_waiting, bound_keycap, canonical, choose_folder, host_setting,
    press, set_theme, until_saved, waiting,
};
pub(in crate::tests) use keys::install_twin;
pub(in crate::tests) use values::{
    aria_label, get_theme, install_plugins, pane_width, plugin_config, plugin_config_of,
    plugin_enabled, run_op, saved, set_mode, set_plugin_config, settings_text, setup_only, tab_to,
    write_plugin,
};
