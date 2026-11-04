//! Web panel files, navigation rules and theme, kept apart so tests run without a native page.
//! The page setup itself is in `page.rs`, shared with the Browser and a plugin `WebView`.

use std::path::{Component, Path};

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

// Only the plugin's own origin passes, and on Windows its `http://kagan-plugin.<id>` form.
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

// Only the page's own files load, plus fetch to the `powers.fetch` hosts.
// A host that fails `host_token` is left out.
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
    let Some(path) = request_path(target) else {
        return refuse(403);
    };
    if path.eq_ignore_ascii_case(THEME_FILE) {
        return Served {
            status: 200,
            mime: mime(THEME_FILE),
            body: if method == "HEAD" {
                Vec::new()
            } else {
                theme.as_bytes().to_vec()
            },
            csp,
        };
    }
    let body = match kg_plugins::read_plugin_file(root, &path) {
        Ok(body) => body,
        Err(PageFileError::Missing) => return refuse(404),
        Err(PageFileError::TooBig) => return refuse(413),
        Err(PageFileError::Refused) => return refuse(403),
    };
    Served {
        status: 200,
        mime: mime(&path),
        body: if method == "HEAD" { Vec::new() } else { body },
        csp,
    }
}

// Builds the page URL for `file`, encoding spaces, `#` and `?` so they stay in the path.
pub(crate) fn page_url(plugin: &str, file: &str) -> Result<String, String> {
    let mut url = url::Url::parse(&format!("{SCHEME}://{plugin}/"))
        .map_err(|_| format!("plugin id {plugin:?} is not a page origin"))?;
    // `set_path` leaves a bare `%` alone. Encode it so the request path is decoded exactly once.
    url.set_path(&format!("/{}", file.replace('%', "%25")));
    Ok(url.to_string())
}

// Reads the path from the raw URL and decodes it once.
// A URL parser would strip `.` and `..` first, which hides traversal from the check.
fn request_path(target: &str) -> Option<String> {
    let rest = target.split_once("://")?.1;
    let encoded = rest.split_once('/')?.1.split(['?', '#']).next()?;
    if !percent_ok(encoded) {
        return None;
    }
    let decoded = percent_encoding::percent_decode_str(encoded)
        .decode_utf8()
        .ok()?;
    let path = decoded.as_ref();
    if path.is_empty()
        || path.contains('\\')
        || path.contains('\0')
        || path.starts_with('/')
        || Path::new(path).is_absolute()
        || !Path::new(path)
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return None;
    }
    Some(path.to_owned())
}

fn percent_ok(encoded: &str) -> bool {
    let bytes = encoded.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'%' {
            index += 1;
            continue;
        }
        if index + 2 >= bytes.len()
            || !bytes[index + 1].is_ascii_hexdigit()
            || !bytes[index + 2].is_ascii_hexdigit()
        {
            return false;
        }
        index += 3;
    }
    true
}

// The message goes in as a string literal, so its quotes cannot break the script.
pub(crate) fn page_event(message: &str) -> Option<String> {
    let literal = serde_json::to_string(message).ok()?;
    Some(format!(
        "window.dispatchEvent(new CustomEvent(\"kagan\",{{detail:JSON.parse({literal})}}))"
    ))
}

pub(crate) const BRIDGE: &str = r#"(function(){function post(message){var text;try{text=JSON.stringify(message);}catch(e){return;}if(typeof text!=="string")return;window.ipc.postMessage(text);}window.kagan=Object.freeze({post:post});var link=document.getElementById("kagan-theme");if(!link){link=document.createElement("link");link.id="kagan-theme";link.rel="stylesheet";link.href=new URL("/__kagan/theme.css",window.location.href).href;(document.head||document.documentElement).appendChild(link);}})();"#;

// Sets the theme tokens, then changes the stylesheet's `?v=` so the view fetches it again.
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

#[cfg(test)]
#[path = "web_test.rs"]
mod web_test;

// The stylesheet text the web view's protocol callback reads.
//
// That callback is not a GPUI context, so the text cannot live in an entity.
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
