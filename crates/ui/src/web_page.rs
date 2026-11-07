//! One opener for the Browser and for a plugin page.

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering::Relaxed},
};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use gpui_kit::{App, AppContext as _, Context, Entity, IntoElement, Render, Window};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
struct LoadMark {
    armed: AtomicU64,
    started: AtomicU64,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
type LoadSignal = (async_channel::Sender<u64>, Arc<LoadMark>);

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
type LoadWait = (
    async_channel::Sender<u64>,
    async_channel::Receiver<u64>,
    u64,
);

#[cfg(any(test, target_os = "linux", target_os = "macos", target_os = "windows"))]
const RUNTIME: &str = "This page needs the Microsoft Edge WebView2 Runtime. Download it from https://developer.microsoft.com/microsoft-edge/webview2/.";

#[cfg(any(test, target_os = "linux", target_os = "macos", target_os = "windows"))]
fn hresult(code: &str) -> String {
    // `0x` and the digits stay apart in source so view-colors.sh does not treat the HRESULT as a color.
    format!("0x{code}")
}

#[cfg(any(test, target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn failure(error: &str) -> String {
    if error.contains(&hresult("80070002")) || error.contains(&hresult("80040154")) {
        RUNTIME.to_owned()
    } else if error.contains("requires an X11 window") {
        "The browser needs an X11 window. On Wayland, start kagan where DISPLAY is set (XWayland)."
            .to_owned()
    } else {
        format!("The page could not open: {error}")
    }
}

#[cfg(any(test, target_os = "windows"))]
static SWEPT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[cfg(any(test, target_os = "windows"))]
pub(crate) fn page_dir(data: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    if !SWEPT.swap(true, std::sync::atomic::Ordering::SeqCst) {
        sweep_page_dirs(data);
    }
    Ok(tempfile::Builder::new()
        .prefix("page-")
        .tempdir_in(data)?
        .keep())
}

#[cfg(any(test, target_os = "windows"))]
fn sweep_page_dirs(data: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(data) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        if name.to_string_lossy().starts_with("page-") {
            let _ = std::fs::remove_dir_all(entry.path());
        }
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) struct WebPage {
    view: Entity<gpui_webview::WebView>,
    loads: async_channel::Sender<u64>,
    finished: async_channel::Receiver<u64>,
    mark: Arc<LoadMark>,
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl WebPage {
    pub(crate) fn load_url(&self, url: &str, cx: &mut App) {
        self.view.update(cx, |view, _| view.load_url(url));
    }

    pub(crate) fn loading(&self) -> LoadWait {
        (
            self.loads.clone(),
            self.finished.clone(),
            self.mark.armed.load(Relaxed),
        )
    }

    pub(crate) fn arm(&self) -> u64 {
        let epoch = self.mark.armed.fetch_add(1, Relaxed).wrapping_add(1);
        while self.finished.try_recv().is_ok() {}
        epoch
    }

    /// Runs one host script in the page. Its reply arrives later, as JSON text.
    pub(crate) fn eval(
        &self,
        script: &str,
        reply: async_channel::Sender<Option<String>>,
        cx: &App,
    ) -> Result<(), String> {
        self.view
            .read(cx)
            .raw()
            .evaluate_script_with_callback(script, move |value| {
                let _ = reply.try_send(Some(value));
            })
            .map_err(|error| error.to_string())
    }

    /// The address the page reports, or an empty string when it has none.
    pub(crate) fn url(&self, cx: &App) -> String {
        self.view.read(cx).raw().url().unwrap_or_default()
    }

    /// Stops a page's `alert`, `confirm` and `prompt` from blocking the agent's command.
    #[cfg(target_os = "windows")]
    pub(crate) fn quiet_dialogs(&self, cx: &App) -> Result<(), String> {
        use wry::WebViewExtWindows as _;
        let webview = self.view.read(cx).raw().webview();
        // SAFETY: the WebView2 settings are read on the UI thread that owns the page.
        let settings = unsafe { webview.Settings() }.map_err(|error| error.to_string())?;
        // SAFETY: the settings object belongs to this page and stays alive for the call.
        unsafe { settings.SetAreDefaultScriptDialogsEnabled(false) }
            .map_err(|error| error.to_string())
    }

    pub(crate) fn back(&self, cx: &mut App) -> Result<(), String> {
        self.view
            .update(cx, |view, _| view.back())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn forward(&self, cx: &mut App) -> Result<(), String> {
        self.view
            .update(cx, |view, _| view.forward())
            .map_err(|error| error.to_string())
    }

    pub(crate) fn reload(&self, cx: &App) -> Result<(), String> {
        self.view
            .read(cx)
            .raw()
            .reload()
            .map_err(|error| error.to_string())
    }

    pub(crate) fn show(&self, cx: &mut App) {
        self.view.update(cx, |view, _| view.show());
    }

    pub(crate) fn hide(&self, cx: &mut App) {
        self.view.update(cx, |view, _| view.hide());
    }

    pub(crate) fn visible(&self, cx: &App) -> bool {
        self.view.read(cx).visible()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
impl Render for WebPage {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    configure: impl for<'a> FnOnce(wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a>,
) -> Result<Entity<WebPage>, String> {
    let (loads, finished) = async_channel::unbounded();
    let mark = Arc::new(LoadMark {
        armed: AtomicU64::new(0),
        started: AtomicU64::new(0),
    });
    let view = open_view_with_loads(
        window,
        cx,
        configure,
        Some((loads.clone(), Arc::clone(&mark))),
    )?;
    Ok(cx.new(|_| WebPage {
        view,
        loads,
        finished,
        mark,
    }))
}

// The native view alone, for a caller that drives the page itself, such as a plugin web panel.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn open_view(
    window: &mut Window,
    cx: &mut App,
    configure: impl for<'a> FnOnce(wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a>,
) -> Result<Entity<gpui_webview::WebView>, String> {
    open_view_with_loads(window, cx, configure, None)
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn open_view_with_loads(
    window: &mut Window,
    cx: &mut App,
    configure: impl for<'a> FnOnce(wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a>,
    loads: Option<LoadSignal>,
) -> Result<Entity<gpui_webview::WebView>, String> {
    #[cfg(target_os = "windows")]
    {
        open_windows(window, cx, configure, loads)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let builder = with_loads(configure(private(wry::WebViewBuilder::new())), loads);
        let view = build_view(builder, window, cx)?;
        #[cfg(target_os = "linux")]
        restrain(&view, cx);
        Ok(cx.new(|_| view))
    }
}

/// Tells `loads` when the page finishes loading, once per navigation.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn with_loads(
    builder: wry::WebViewBuilder<'_>,
    loads: Option<LoadSignal>,
) -> wry::WebViewBuilder<'_> {
    let Some((loads, mark)) = loads else {
        return builder;
    };
    builder.with_on_page_load_handler(move |event, _| match event {
        wry::PageLoadEvent::Started => mark.started.store(mark.armed.load(Relaxed), Relaxed),
        wry::PageLoadEvent::Finished => {
            let _ = loads.try_send(mark.started.load(Relaxed));
        }
    })
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn private(builder: wry::WebViewBuilder<'_>) -> wry::WebViewBuilder<'_> {
    builder
        .with_incognito(true)
        .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
        .with_download_started_handler(|_url, _path| false)
}

#[cfg(target_os = "windows")]
fn open_windows(
    window: &mut Window,
    cx: &mut App,
    configure: impl for<'a> FnOnce(wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a>,
    loads: Option<LoadSignal>,
) -> Result<Entity<gpui_webview::WebView>, String> {
    let data = kg_core::Workspace::global(cx)
        .read(cx)
        .data_dir()
        .to_owned();
    let storage = page_dir(&data).map_err(|error| failure(&error.to_string()))?;
    let mut context = wry::WebContext::new(Some(storage.clone()));
    let builder = configure(private(wry::WebViewBuilder::new_with_web_context(
        &mut context,
    )));
    let mut view = match build_view(with_loads(builder, loads), window, cx) {
        Ok(view) => view,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&storage);
            return Err(error);
        }
    };
    view.remove_user_data_when_closed(storage);
    Ok(cx.new(|_| view))
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn build_view(
    builder: wry::WebViewBuilder<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Result<gpui_webview::WebView, String> {
    gpui_webview::WebView::build(builder, window, cx).map_err(|error| failure(&error.to_string()))
}

// Linux pages cannot use the camera or microphone, and a script dialog cannot stall the window.
#[cfg(target_os = "linux")]
fn restrain(view: &gpui_webview::WebView, cx: &App) {
    use webkit2gtk::{
        FileChooserRequestExt as _, PermissionRequestExt as _, SettingsExt as _, WebViewExt as _,
    };
    use wry::WebViewExtUnix as _;

    let page = view.raw().webview();
    if let Some(settings) = page.settings() {
        settings.set_allow_modal_dialogs(false);
        settings.set_enable_media_stream(false);
        settings.set_enable_webrtc(false);
        settings.set_javascript_can_open_windows_automatically(false);
    }
    page.connect_permission_request(|_, request| {
        request.deny();
        true
    });
    page.connect_script_dialog(|_, dialog| {
        match dialog.dialog_type() {
            webkit2gtk::ScriptDialogType::Confirm
            | webkit2gtk::ScriptDialogType::BeforeUnloadConfirm => {
                dialog.confirm_set_confirmed(false);
            }
            webkit2gtk::ScriptDialogType::Prompt => dialog.prompt_set_text(""),
            _ => {}
        }
        true
    });
    page.connect_run_file_chooser(|_, request| {
        request.cancel();
        true
    });
    let mode = crate::theme::ThemeState::global(cx).read(cx).mode;
    let native = match mode {
        gpui_kit::component::ThemeMode::Light => wry::Theme::Light,
        gpui_kit::component::ThemeMode::Dark => wry::Theme::Dark,
    };
    let _ = view.set_theme(native);
}

// A per-plugin `WKWebsiteDataStore` needs macOS 14 or later. Older versions keep the page private.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) const STORE_NEEDS_MACOS: &str = "Saving page data needs macOS 14 or later.";

#[cfg(target_os = "macos")]
pub(crate) use kg_plugins::macos_major;

// Keeps the store id only on macOS 14 or later. Older systems get None, so the page stays private.
#[cfg(target_os = "macos")]
#[must_use]
pub(crate) fn durable_store(store: Option<[u8; 16]>, major: u32) -> Option<[u8; 16]> {
    store.filter(|_| major >= 14)
}

// Gives a page its own durable store. Run it after `page::private`: a durable store needs
// incognito off, and the last call wins.
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
pub(crate) fn with_store(
    builder: wry::WebViewBuilder<'_>,
    store: Option<[u8; 16]>,
) -> wry::WebViewBuilder<'_> {
    #[cfg(target_os = "macos")]
    {
        use wry::WebViewBuilderExtDarwin as _;
        match durable_store(store, macos_major()) {
            Some(id) => builder.with_incognito(false).with_data_store_identifier(id),
            None => builder,
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = store;
        builder
    }
}

#[cfg(test)]
#[path = "web_page_test.rs"]
mod tests;
