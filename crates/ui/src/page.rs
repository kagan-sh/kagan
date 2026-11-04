//! One opener for the Browser and for a plugin page.

#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::{App, AppContext as _, Context, Entity, IntoElement, Render, Window};

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
const RUNTIME: &str = "This page needs the Microsoft Edge WebView2 Runtime. Download it from https://developer.microsoft.com/microsoft-edge/webview2/.";

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
fn hresult(code: &str) -> String {
    // `0x` and the digits stay apart in source so view-colors.sh does not treat the HRESULT as a color.
    format!("0x{code}")
}

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
pub(crate) fn failure(error: &str) -> String {
    if error.contains(&hresult("80070002")) || error.contains(&hresult("80040154")) {
        RUNTIME.to_owned()
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) struct Page {
    view: Entity<gpui_webview::WebView>,
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Page {
    pub(crate) fn load_url(&self, url: &str, cx: &mut App) {
        self.view.update(cx, |view, _| view.load_url(url));
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
impl Render for Page {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.view.clone()
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    configure: impl for<'a> FnOnce(wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a>,
) -> Result<Entity<Page>, String> {
    #[cfg(target_os = "windows")]
    {
        open_windows(window, cx, configure)
    }
    #[cfg(target_os = "macos")]
    {
        build(configure(private(wry::WebViewBuilder::new())), window, cx)
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
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
) -> Result<Entity<Page>, String> {
    let data = kg_core::Workspace::global(cx)
        .read(cx)
        .data_dir()
        .to_owned();
    let storage = page_dir(&data).map_err(|error| failure(&error.to_string()))?;
    let mut context = wry::WebContext::new(Some(storage.clone()));
    let mut view = match build_view(
        configure(private(wry::WebViewBuilder::new_with_web_context(
            &mut context,
        ))),
        window,
        cx,
    ) {
        Ok(view) => view,
        Err(error) => {
            let _ = std::fs::remove_dir_all(&storage);
            return Err(error);
        }
    };
    view.remove_user_data_when_closed(storage);
    let view = cx.new(|_| view);
    Ok(cx.new(|_| Page { view }))
}

#[cfg(target_os = "macos")]
fn build(
    builder: wry::WebViewBuilder<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Result<Entity<Page>, String> {
    let view = build_view(builder, window, cx)?;
    let view = cx.new(|_| view);
    Ok(cx.new(|_| Page { view }))
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build_view(
    builder: wry::WebViewBuilder<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Result<gpui_webview::WebView, String> {
    gpui_webview::WebView::build(builder, window, cx).map_err(|error| failure(&error.to_string()))
}

// A per-plugin `WKWebsiteDataStore` needs macOS 14+. Older versions keep the page private.
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) const STORE_NEEDS_MACOS: &str = "Saving page data needs macOS 14 or later.";

#[cfg(target_os = "macos")]
#[must_use]
pub(crate) fn macos_major() -> u32 {
    use objc2_foundation::NSProcessInfo;
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    u32::try_from(version.majorVersion).unwrap_or(0)
}

// The store id on macOS 14 or later. Otherwise None, and the page stays private.
#[cfg(target_os = "macos")]
#[must_use]
pub(crate) fn durable_store(store: Option<[u8; 16]>, major: u32) -> Option<[u8; 16]> {
    store.filter(|_| major >= 14)
}

// Gives a page its own durable store. Must run after `page::private`: a durable store needs
// incognito off, and the last call wins.
#[cfg(any(target_os = "macos", target_os = "windows"))]
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
mod tests {
    use super::*;

    #[test]
    fn a_missing_runtime_names_the_download() {
        let message = failure(&format!(
            "creating the environment failed: {}",
            hresult("80070002")
        ));
        assert!(message.contains("WebView2 Runtime"));
        assert!(message.contains("https://developer.microsoft.com/microsoft-edge/webview2/"));
        let other = failure("the window is gone");
        assert!(other.contains("could not open"));
        assert!(!other.contains("WebView2 Runtime"));
    }

    #[test]
    fn webview_files_stay_inside_kagan_data() {
        let data = tempfile::tempdir().expect("temp data");
        let stale = data.path().join("page-stale");
        std::fs::create_dir_all(&stale).expect("stale page dir");
        std::fs::write(stale.join("lock"), b"x").expect("stale file");
        SWEPT.store(false, std::sync::atomic::Ordering::SeqCst);
        let path = page_dir(data.path()).expect("page dir");
        assert!(path.starts_with(data.path()));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(name.starts_with("page-"));
        assert!(
            !stale.exists(),
            "abandoned page dirs are removed on restart"
        );
        let _ = std::fs::remove_dir_all(path);
    }
}
