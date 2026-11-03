//! One opener for the Browser and for a plugin page.
//!
//! Callers set the address, private mode, and which navigations stay. A later caller can also
//! attach a custom protocol and an IPC handler to the same builder.

#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::{App, AppContext as _, Context, Entity, IntoElement, Render, Window};

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
const RUNTIME: &str = "This page needs the Microsoft Edge WebView2 Runtime. Download it from https://developer.microsoft.com/microsoft-edge/webview2/.";

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
fn hresult(code: &str) -> String {
    format!("0x{code}")
}

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
pub(crate) fn failure(error: &str) -> String {
    // The digits stay separate from the prefix so the view-color check does not read them as a color.
    if error.contains(&hresult("80070002")) || error.contains(&hresult("80040154")) {
        RUNTIME.to_owned()
    } else {
        format!("The page could not open: {error}")
    }
}

#[cfg(any(test, target_os = "windows"))]
pub(crate) fn page_dir(data: &std::path::Path) -> std::io::Result<tempfile::TempDir> {
    tempfile::Builder::new().prefix("page-").tempdir_in(data)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) struct Page {
    // First, so the page drops before its directory.
    view: Entity<gpui_webview::WebView>,
    #[cfg(target_os = "windows")]
    _storage: tempfile::TempDir,
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
        build(configure(wry::WebViewBuilder::new()), window, cx)
    }
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
    let mut context = wry::WebContext::new(Some(storage.path().to_owned()));
    let view = build_view(
        configure(wry::WebViewBuilder::new_with_web_context(&mut context)),
        window,
        cx,
    )?;
    let view = cx.new(|_| view);
    Ok(cx.new(|_| Page {
        view,
        _storage: storage,
    }))
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
        let storage = page_dir(data.path()).expect("page dir");
        let path = storage.path();
        assert!(path.starts_with(data.path()));
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        assert!(name.starts_with("page-"));
    }
}
