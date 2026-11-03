//! One opener for the Browser and for a plugin page.
//!
//! Callers set the address, private mode, and which navigations stay. A later caller can also
//! attach a custom protocol and an IPC handler to the same builder.

use gpui_kit::{App, AppContext as _, Entity, Window};

const RUNTIME: &str = "This page needs the Microsoft Edge WebView2 Runtime. Download it from https://developer.microsoft.com/microsoft-edge/webview2/.";

pub(crate) fn failure(error: &str) -> String {
    if error.contains("0x80070002") || error.contains("0x80040154") {
        RUNTIME.to_owned()
    } else {
        format!("The page could not open: {error}")
    }
}

#[cfg(any(test, target_os = "windows"))]
pub(crate) fn webview_storage(data: &std::path::Path) -> std::path::PathBuf {
    data.join("webview")
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(crate) fn open(
    window: &mut Window,
    cx: &mut App,
    configure: impl for<'a> FnOnce(wry::WebViewBuilder<'a>) -> wry::WebViewBuilder<'a>,
) -> Result<Entity<gpui_webview::WebView>, String> {
    #[cfg(target_os = "windows")]
    {
        return open_windows(window, cx, configure);
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
) -> Result<Entity<gpui_webview::WebView>, String> {
    let dir = webview_storage(kg_core::Workspace::global(cx).read(cx).data_dir());
    std::fs::create_dir_all(&dir).map_err(|error| failure(&error.to_string()))?;
    let mut context = wry::WebContext::new(Some(dir));
    build(
        configure(wry::WebViewBuilder::new_with_web_context(&mut context)),
        window,
        cx,
    )
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
fn build(
    builder: wry::WebViewBuilder<'_>,
    window: &mut Window,
    cx: &mut App,
) -> Result<Entity<gpui_webview::WebView>, String> {
    match gpui_webview::WebView::build(builder, window, cx) {
        Ok(view) => Ok(cx.new(|_| view)),
        Err(error) => Err(failure(&error.to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::{failure, webview_storage};

    #[test]
    fn a_missing_runtime_names_the_download() {
        let missing = failure("WebView2 error: WindowsError(Error { code: HRESULT(0x80070002) })");
        assert!(missing.contains("https://developer.microsoft.com/microsoft-edge/webview2/"));
        assert!(missing.contains("WebView2 Runtime"));
        assert!(failure("class not registered 0x80040154").contains("WebView2 Runtime"));
        assert_eq!(
            failure("something else"),
            "The page could not open: something else"
        );
    }

    #[test]
    fn webview_files_stay_inside_kagan_data() {
        let data = std::path::Path::new("kagan-data");
        let storage = webview_storage(data);
        assert_eq!(storage, data.join("webview"));
        assert!(storage.starts_with(data));
    }
}
