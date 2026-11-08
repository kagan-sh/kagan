use anyhow::{Context as _, Result};
use async_channel::Sender;
use gpui_kit::QuitMode;
use kg_domain::{Op, OpError, OpResult};

use crate::socket;

type Request = (Op, kg_core::Requester, Sender<Result<OpResult, OpError>>);

struct Opened;
impl gpui_kit::Global for Opened {}

// WebKitGTK embeds only in an X11 window. When `DISPLAY` is set, including XWayland, the
// window uses X11 so the browser can sit in it. With no X11 display the window stays on
// Wayland or headless, and opening a page says that it needs X11.
#[cfg(target_os = "linux")]
fn desktop() -> gpui_kit::Application {
    let display = std::env::var_os("DISPLAY").is_some_and(|value| !value.is_empty());
    let modes = if display {
        gpui_kit::WindowingModes::X11 | gpui_kit::WindowingModes::HEADLESS
    } else {
        gpui_kit::WindowingModes::all()
    };
    gpui_kit::platform::linux(modes)
}

#[cfg(not(target_os = "linux"))]
fn desktop() -> gpui_kit::Application {
    gpui_kit::application()
}

pub(crate) fn run() -> Result<()> {
    let location = socket::location()?;
    let Some(listener) = socket::open(&location).context("cannot open the kg socket")? else {
        return Ok(());
    };
    let (requests, incoming) = async_channel::unbounded::<Request>();
    socket::serve(listener, move |op, gone| {
        let (reply, replied) = async_channel::bounded(1);
        requests
            .send_blocking((op, kg_core::Requester::new(gone.clone()), reply))
            .map_err(|_| OpError::Failed("kagan is quitting".into()))?;
        replied
            .recv_blocking()
            .map_err(|_| OpError::Failed("kagan dropped the request".into()))?
    });
    let data_dir = if let Some(dir) = std::env::var_os("KG_DATA_DIR") {
        std::path::PathBuf::from(dir)
    } else {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .context("HOME or USERPROFILE is not set")?;
        std::path::Path::new(&home).join(".kagan")
    };
    let cli = std::env::current_exe()?;
    kg_process::warm_user_path();
    let components = component_catalog()?;
    let app = desktop()
        .with_assets(kg_ui::Assets)
        .with_quit_mode(QuitMode::Explicit);
    app.on_reopen(|cx| {
        // Before the workspace opens there is no window to show, only the start failure.
        if !cx.has_global::<Opened>() {
            return;
        }
        if let Err(error) = kg_ui::open_window(cx) {
            eprintln!("kg: {error}");
        }
    });
    app.run(move |cx| {
        #[cfg(target_os = "macos")]
        crate::icon::install();
        let open = kg_core::Workspace::open(data_dir, cli, Some(location), cx);
        cx.spawn(async move |cx| {
            if let Err(error) = open.await {
                eprintln!("kg: {error}");
                cx.update(|cx| {
                    if let Err(shown) = kg_ui::show_start_failure(error.to_string(), cx) {
                        eprintln!("kg: {shown}");
                        cx.quit();
                    }
                });
                while let Ok((_, _, reply)) = incoming.recv().await {
                    // The client may have hung up already.
                    let _ = reply.send(Err(error.clone())).await;
                }
                return;
            }
            let (_open_window, plugins) = cx.update(|cx| {
                cx.set_global(Opened);
                gpui_kit::init(cx);
                kg_ui::init(cx);
                let open_window = cx.subscribe(&kg_core::Workspace::global(cx), |_, event, cx| {
                    if matches!(event, kg_core::WorkspaceEvent::Open)
                        && let Err(error) = kg_ui::open_window(cx)
                    {
                        eprintln!("kg: {error}");
                    }
                });
                (open_window, kg_plugins::init(cx, components))
            });
            if let Err(error) = plugins.await {
                eprintln!("kg: plugins: {error}");
            }
            cx.update(kg_ui::plugins_ready);
            while let Ok((op, requester, reply)) = incoming.recv().await {
                let task =
                    cx.update(|cx| kg_core::Workspace::dispatch_for(op, Some(requester), cx));
                cx.spawn(async move |_| reply.send(task.await).await)
                    .detach();
            }
        })
        .detach();
    });
    Ok(())
}

fn component_catalog() -> Result<gpui_shell::FrozenComponentRegistry> {
    let mut registry = gpui_shell::components::registry()
        .context("cannot build the gpui-component catalog")?
        .with_window_opener(open_plugin_window);
    kg_ui::register_components(&mut registry).context("cannot register kagan components")?;
    registry
        .freeze()
        .context("cannot freeze the component catalog")
}

// gpui-component overlays locate their host with `window.root::<Root>()` and panic
// when the runtime roots the window at anything else.
fn open_plugin_window(
    cx: &mut gpui_shell::gpui::App,
    options: gpui_shell::gpui::WindowOptions,
    build: &mut dyn FnMut(
        &mut gpui_shell::gpui::Window,
        &mut gpui_shell::gpui::App,
    ) -> gpui_shell::gpui::AnyView,
) -> gpui_shell::anyhow::Result<gpui_shell::gpui::AnyWindowHandle> {
    use gpui_shell::gpui::AppContext as _;

    let handle = cx.open_window(kg_ui::prepare_window(options), |window, cx| {
        let inner = build(window, cx);
        cx.new(|cx| gpui_kit::component::Root::new(inner, window, cx))
    })?;
    Ok(handle.into())
}

#[cfg(test)]
mod tests {
    #[test]
    fn catalog_includes_gpui_component_and_kagan() {
        let catalog = super::component_catalog().unwrap();
        let names: Vec<_> = catalog
            .descriptors()
            .map(gpui_shell::ComponentDescriptor::name)
            .collect();
        for name in ["Spinner", "ThreadTitle", "ThreadStatus", "Markdown"] {
            assert!(names.contains(&name), "{name} missing from {names:?}");
        }
        assert!(catalog.initializer().is_some());
        assert!(catalog.window_opener().is_some());
    }
}
