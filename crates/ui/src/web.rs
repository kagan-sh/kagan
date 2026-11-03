//! One wry page setup, shared by the Browser, a plugin `WebView`, and a web panel.
//!
//! Private storage, no new window, and no download. Camera and microphone stay denied by the
//! pinned wry patch. A web panel's files and navigation rules live here too, so they can be
//! tested without a native page.

use std::path::Path;

use kg_plugins::{PageFileError, SCHEME};

use crate::theme::ThemeState;

#[cfg_attr(target_os = "macos", allow(dead_code))]
pub(crate) const ONLY_MACOS: &str = "This page works on macOS only for now.";

const THEME_FILE: &str = "__kagan/theme.css";

pub(crate) struct Served {
    pub(crate) status: u16,
    pub(crate) mime: &'static str,
    pub(crate) body: Vec<u8>,
    pub(crate) csp: String,
}

/// Whether a navigation may proceed. Only the plugin's own page origin is allowed.
pub(crate) fn allows_navigation(plugin: &str, target: &str) -> bool {
    let Ok(url) = url::Url::parse(target) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return false;
    }
    own_origin(plugin, &url)
}

fn own_origin(plugin: &str, url: &url::Url) -> bool {
    let host = url.host_str();
    if url.scheme() == SCHEME {
        return host == Some(plugin);
    }
    windows_origin(plugin, url.scheme(), host)
}

#[cfg(target_os = "windows")]
fn windows_origin(plugin: &str, scheme: &str, host: Option<&str>) -> bool {
    let expected = format!("{SCHEME}.{plugin}");
    scheme == "http" && host == Some(expected.as_str())
}

#[cfg(not(target_os = "windows"))]
fn windows_origin(_plugin: &str, _scheme: &str, _host: Option<&str>) -> bool {
    false
}

/// CSP for a plugin page. Scripts, images and styles come from the page itself.
/// `connect-src` lists only `powers.fetch` hosts.
pub(crate) fn csp(hosts: &[String]) -> String {
    let mut connect = String::from("'self'");
    for host in hosts {
        if host_token(host) {
            connect.push_str(" https://");
            connect.push_str(host);
            connect.push_str(" http://");
            connect.push_str(host);
        }
    }
    format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; font-src 'self'; connect-src {connect}; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'"
    )
}

pub(crate) fn mime(path: &str) -> &'static str {
    let extension = path
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "wasm" => "application/wasm",
        _ => "application/octet-stream",
    }
}

pub(crate) fn theme_css(cx: &gpui_kit::App) -> String {
    let state = ThemeState::global(cx);
    let state = state.read(cx);
    let palette = &state.palette;
    let scheme = match state.mode {
        gpui_kit::component::ThemeMode::Light => "light",
        gpui_kit::component::ThemeMode::Dark => "dark",
    };
    let hover = palette.hover.unwrap_or(palette.raised);
    format!(
        ":root{{--bg:{bg};--surface:{surface};--raised:{raised};--hover:{hover};--ink:{ink};--muted:{muted};--line:{line};--accent:{accent};--needs:{needs};--failed:{failed};--added:{added};--working:{working};color-scheme:{scheme}}}\n",
        bg = palette.bg.hex(),
        surface = palette.surface.hex(),
        raised = palette.raised.hex(),
        hover = hover.hex(),
        ink = palette.ink.hex(),
        muted = palette.muted.hex(),
        line = palette.line.hex(),
        accent = palette.accent.hex(),
        needs = palette.needs.hex(),
        failed = palette.failed.hex(),
        added = palette.added().hex(),
        working = palette.working.hex(),
    )
}

pub(crate) fn serve(
    root: &Path,
    plugin: &str,
    method: &str,
    target: &str,
    hosts: &[String],
    theme: &str,
) -> Served {
    let csp = csp(hosts);
    let refuse = |status| Served {
        status,
        mime: "text/plain; charset=utf-8",
        body: Vec::new(),
        csp: csp.clone(),
    };
    if !matches!(method, "GET" | "HEAD") {
        return refuse(405);
    }
    if !allows_navigation(plugin, target) {
        return refuse(403);
    }
    let Ok(url) = url::Url::parse(target) else {
        return refuse(403);
    };
    let path = url.path().trim_start_matches('/');
    if path == THEME_FILE {
        return Served {
            status: 200,
            mime: mime(path),
            body: if method == "HEAD" {
                Vec::new()
            } else {
                theme.as_bytes().to_vec()
            },
            csp,
        };
    }
    let body = match kg_plugins::read_plugin_file(root, path) {
        Ok(body) => body,
        Err(PageFileError::Missing) => return refuse(404),
        Err(PageFileError::TooBig) => return refuse(413),
        Err(PageFileError::Refused) => return refuse(403),
    };
    Served {
        status: 200,
        mime: mime(path),
        body: if method == "HEAD" { Vec::new() } else { body },
        csp,
    }
}

/// The script that tells the page one JSON value, as a `kagan` event.
pub(crate) fn page_event(message: &str) -> Option<String> {
    let literal = serde_json::to_string(message).ok()?;
    Some(format!(
        "window.dispatchEvent(new CustomEvent(\"kagan\",{{detail:JSON.parse({literal})}}))"
    ))
}

pub(crate) const BRIDGE: &str = r#"(function(){function post(message){var text;try{text=JSON.stringify(message);}catch(e){return;}if(typeof text!=="string")return;window.ipc.postMessage(text);}window.kagan=Object.freeze({post:post});var link=document.getElementById("kagan-theme");if(!link){link=document.createElement("link");link.id="kagan-theme";link.rel="stylesheet";link.href=new URL("/__kagan/theme.css",window.location.href).href;(document.head||document.documentElement).appendChild(link);}})();"#;

/// Applies the live Workbench tokens in the page and reloads `/__kagan/theme.css`.
pub(crate) fn theme_refresh(cx: &gpui_kit::App) -> String {
    let state = ThemeState::global(cx);
    let state = state.read(cx);
    let palette = &state.palette;
    let scheme = match state.mode {
        gpui_kit::component::ThemeMode::Light => "light",
        gpui_kit::component::ThemeMode::Dark => "dark",
    };
    let hover = palette.hover.unwrap_or(palette.raised);
    format!(
        concat!(
            "(function(){{var r=document.documentElement;",
            "var v={{'--bg':'{bg}','--surface':'{surface}','--raised':'{raised}',",
            "'--hover':'{hover}','--ink':'{ink}','--muted':'{muted}','--line':'{line}',",
            "'--accent':'{accent}','--needs':'{needs}','--failed':'{failed}',",
            "'--added':'{added}','--working':'{working}'}};",
            "for(var k in v)r.style.setProperty(k,v[k]);",
            "r.style.colorScheme='{scheme}';",
            "var link=document.getElementById('kagan-theme');",
            "if(link){{link.href=link.href.split('?')[0]+'?v='+Date.now();",
            "link.dispatchEvent(new Event('load'));}}}})();"
        ),
        bg = palette.bg.hex(),
        surface = palette.surface.hex(),
        raised = palette.raised.hex(),
        hover = hover.hex(),
        ink = palette.ink.hex(),
        muted = palette.muted.hex(),
        line = palette.line.hex(),
        accent = palette.accent.hex(),
        needs = palette.needs.hex(),
        failed = palette.failed.hex(),
        added = palette.added().hex(),
        working = palette.working.hex(),
        scheme = scheme,
    )
}

fn host_token(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && !host.starts_with('.')
        && !host.ends_with('.')
        && !host.contains("..")
        && host
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
}

#[cfg(target_os = "macos")]
pub(crate) fn builder(
    url: &str,
    allow: impl Fn(String) -> bool + 'static,
) -> wry::WebViewBuilder<'static> {
    wry::WebViewBuilder::new()
        .with_url(url)
        .with_incognito(true)
        .with_navigation_handler(allow)
        .with_new_window_req_handler(|_url, _features| wry::NewWindowResponse::Deny)
        .with_download_started_handler(|_url, _path| false)
}

#[cfg(test)]
#[path = "web_test.rs"]
mod web_test;

/// The stylesheet text the web view's protocol callback reads.
///
/// That callback is not a GPUI context, so the text cannot live in an entity.
#[cfg(target_os = "macos")]
mod theme_cell {
    #![allow(clippy::disallowed_types)]

    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub(crate) struct ThemeCss(Arc<Mutex<String>>);

    impl ThemeCss {
        pub(crate) fn new(css: String) -> Self {
            Self(Arc::new(Mutex::new(css)))
        }

        pub(crate) fn get(&self) -> String {
            match self.0.lock() {
                Ok(guard) => guard.clone(),
                Err(poison) => poison.into_inner().clone(),
            }
        }

        pub(crate) fn set(&self, css: String) {
            match self.0.lock() {
                Ok(mut guard) => *guard = css,
                Err(poison) => *poison.into_inner() = css,
            }
        }
    }
}

#[cfg(target_os = "macos")]
pub(crate) use theme_cell::ThemeCss;
