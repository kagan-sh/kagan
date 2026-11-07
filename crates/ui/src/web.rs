//! Web panel files, navigation rules and theme, kept apart so tests run without a native page.
//! The page setup itself is in `web_page.rs`, the opener shared with the Browser and a plugin
//! `WebView`. This module compiles wherever a page is drawn, and in tests, so the rules run on
//! every system CI covers.

use std::path::{Component, Path};

use kg_plugins::{PageFileError, SCHEME};

#[cfg(desktop)]
use crate::theme::ThemeState;

const THEME_FILE: &str = "__kagan/theme.css";

pub(crate) struct Served {
    pub(crate) status: u16,
    pub(crate) mime: &'static str,
    pub(crate) body: Vec<u8>,
    pub(crate) csp: String,
}

// Only the plugin's own origin passes.
pub(crate) fn allows_navigation(plugin: &str, target: &str) -> bool {
    let target = unwork(target);
    let Ok(url) = url::Url::parse(&target) else {
        return false;
    };
    if !url.username().is_empty() || url.password().is_some() || url.port().is_some() {
        return false;
    }
    url.scheme() == SCHEME && url.host_str() == Some(plugin)
}

// WebView2 serves `kagan-plugin://<id>/` as `http://kagan-plugin.<id>/`, and navigation reports
// that form (decision 142). The protocol handler gets the original back.
#[cfg(any(test, target_os = "windows"))]
fn unwork(target: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;
    let Some(rest) = target
        .strip_prefix("http://")
        .and_then(|rest| rest.strip_prefix(SCHEME))
        .and_then(|rest| rest.strip_prefix('.'))
    else {
        return Cow::Borrowed(target);
    };
    let (host, tail) = rest.split_at(rest.find(['/', '?', '#']).unwrap_or(rest.len()));
    Cow::Owned(format!("{SCHEME}://{host}{tail}"))
}

#[cfg(not(any(test, target_os = "windows")))]
fn unwork(target: &str) -> std::borrow::Cow<'_, str> {
    std::borrow::Cow::Borrowed(target)
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

#[cfg(desktop)]
pub(crate) fn theme_css(cx: &gpui_kit::App) -> String {
    let state = ThemeState::global(cx);
    let state = state.read(cx);
    stylesheet(&tokens(&state.palette, &state.look()), scheme(state.mode))
}

fn stylesheet(tokens: &[(&'static str, String)], scheme: &str) -> String {
    let declarations = tokens
        .iter()
        .map(|(name, value)| format!("{name}:{value}"))
        .collect::<Vec<_>>()
        .join(";");
    format!(":root{{{declarations};color-scheme:{scheme}}}\n")
}

#[cfg(desktop)]
fn scheme(mode: gpui_kit::component::ThemeMode) -> &'static str {
    match mode {
        gpui_kit::component::ThemeMode::Light => "light",
        gpui_kit::component::ThemeMode::Dark => "dark",
    }
}

#[cfg(desktop)]
fn tokens(palette: &kg_domain::Palette, look: &kg_domain::Look) -> Vec<(&'static str, String)> {
    let hover = palette.hover.unwrap_or(palette.raised);
    let px = |value: u16| format!("{value}px");
    let mut tokens: Vec<(&'static str, String)> = [
        ("--bg", palette.bg),
        ("--surface", palette.surface),
        ("--raised", palette.raised),
        ("--hover", hover),
        ("--ink", palette.ink),
        ("--muted", palette.muted),
        ("--line", palette.line),
        ("--accent", palette.accent),
        ("--needs", palette.needs),
        ("--failed", palette.failed),
        ("--added", palette.added()),
        ("--working", palette.working),
        ("--selection", palette.selection()),
        ("--input", palette.input()),
    ]
    .into_iter()
    .map(|(name, color)| (name, color.hex()))
    .collect();
    tokens.extend([
        ("--space-xs", px(look.space.xs)),
        ("--space-sm", px(look.space.sm)),
        ("--space-md", px(look.space.md)),
        ("--space-lg", px(look.space.lg)),
        ("--space-xl", px(look.space.xl)),
        ("--space-xxl", px(look.space.xxl)),
        ("--radius-xs", px(look.radius.xs)),
        ("--radius-sm", px(look.radius.sm)),
        ("--radius-md", px(look.radius.md)),
        ("--radius-lg", px(look.radius.lg)),
        ("--text-caption", px(look.type_sizes.caption)),
        ("--text-small", px(look.type_sizes.small)),
        ("--text-body", px(look.type_sizes.body)),
        ("--text-title", px(look.type_sizes.title)),
        ("--text-mono", px(look.type_sizes.mono)),
        ("--row", px(look.row)),
        ("--control", px(look.control)),
    ]);
    tokens
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

#[cfg(desktop)]
pub(crate) const BRIDGE: &str = r#"(function(){function post(message){var text;try{text=JSON.stringify(message);}catch(e){return;}if(typeof text!=="string")return;window.ipc.postMessage(text);}window.kagan=Object.freeze({post:post});var link=document.getElementById("kagan-theme");if(!link){link=document.createElement("link");link.id="kagan-theme";link.rel="stylesheet";link.href=new URL("/__kagan/theme.css",window.location.href).href;(document.head||document.documentElement).appendChild(link);}})();"#;

// Sets the theme tokens, then changes the stylesheet's `?v=` so the view fetches it again.
#[cfg(desktop)]
pub(crate) fn theme_refresh(cx: &gpui_kit::App) -> String {
    let state = ThemeState::global(cx);
    let state = state.read(cx);
    refresh(&tokens(&state.palette, &state.look()), scheme(state.mode))
}

fn refresh(tokens: &[(&'static str, String)], scheme: &str) -> String {
    let vars = tokens
        .iter()
        .map(|(name, value)| format!("'{name}':'{value}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        concat!(
            "(function(){{var r=document.documentElement;",
            "var v={{{vars}}};",
            "for(var k in v)r.style.setProperty(k,v[k]);",
            "r.style.colorScheme='{scheme}';",
            "var link=document.getElementById('kagan-theme');",
            "if(link){{link.href=link.href.split('?')[0]+'?v='+Date.now();",
            "link.dispatchEvent(new Event('load'));}}}})();"
        ),
        vars = vars,
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
mod tests;

// The stylesheet text the web view's protocol callback reads.
//
// That callback is not a GPUI context, so the text cannot live in an entity.
#[cfg(desktop)]
mod theme_cell {
    #![expect(
        clippy::disallowed_types,
        reason = "the WebKit callback needs a std Arc<Mutex> because it has no GPUI context"
    )]

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

#[cfg(desktop)]
pub(crate) use theme_cell::ThemeCss;
