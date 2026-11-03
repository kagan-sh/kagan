mod about;
mod brand;
mod components;
mod failed;
mod kagan;
mod plugins;
mod synthetic;
mod terminal;
#[cfg(test)]
mod tests;
mod theme;
mod ui_dispatch;
mod ui_driver;
mod ui_observe;
mod ui_registry;
mod web;

use gpui_kit::App;

pub use brand::{Assets, prepare_window};

/// Sets up the Workbench theme. Call once, after `kg_core::Workspace::open`
/// and `gpui_kit::init`, before the first [`open_window`].
pub fn init(cx: &mut App) {
    theme::load_fonts(cx);
    theme::init(cx);
    // A popover binds space to confirm, which would swallow it while typing in a list's search.
    cx.bind_keys([gpui_kit::KeyBinding::new(
        "space",
        gpui_kit::NoAction,
        Some("List > Input"),
    )]);
    kagan::install_keys(cx);
    let workspace = kg_core::Workspace::global(cx);
    cx.subscribe(&workspace, |_, event, cx| {
        let result = match event {
            kg_core::WorkspaceEvent::Settings => kagan::show_settings(cx),
            kg_core::WorkspaceEvent::SettingsPlugins => kagan::show_plugin_settings(cx),
            kg_core::WorkspaceEvent::NewPlugin => kagan::start_plugin(cx),
            _ => return,
        };
        if let Err(error) = result {
            eprintln!("kg: {error}");
        }
    })
    .detach();
    plugins::init(cx);
    kagan::watch_waiting(cx);
    ui_driver::install(cx);
}

/// Shows the Kagan window. An open window comes forward and the app comes to the front; a new
/// window opens only when none is open.
///
/// # Errors
///
/// When the platform cannot open a window.
pub fn open_window(cx: &mut App) -> gpui_kit::Result<()> {
    kagan::show(cx)
}

/// Shows why kagan could not start in a small window. Quit, or closing it, ends the app.
///
/// # Errors
///
/// When the platform cannot open a window.
pub fn show_start_failure(message: String, cx: &mut App) -> gpui_kit::Result<()> {
    gpui_kit::init(cx);
    failed::show(message.into(), cx)
}

/// Registers kagan’s native components in the host catalog.
///
/// # Errors
/// When the catalog already contains a kagan component name.
pub fn register_components(
    registry: &mut gpui_shell::ComponentRegistry,
) -> Result<(), gpui_shell::RegistryError> {
    components::register(registry)
}
