use std::path::Path;

use super::{allows_navigation, csp, mime, page_event, page_url, serve};

const PLUGIN: &str = "dev.kagan.page";

#[test]
fn navigation_stays_on_the_plugin_origin() {
    assert!(allows_navigation(
        PLUGIN,
        "kagan-plugin://dev.kagan.page/ui/index.html"
    ));
    assert!(!allows_navigation(PLUGIN, "https://example.com/"));
    assert!(!allows_navigation(PLUGIN, "https://files.example/draw"));
    assert!(!allows_navigation(
        PLUGIN,
        "kagan-plugin://dev.other.page/ui/index.html"
    ));
    assert!(!allows_navigation(PLUGIN, "file:///etc/passwd"));
    assert!(!allows_navigation(PLUGIN, "javascript:alert(1)"));
    assert!(!allows_navigation(PLUGIN, "about:blank"));
}

#[test]
fn fetch_hosts_are_only_a_connect_source() {
    let policy = csp(&["files.example".into(), "not a host".into()]);
    assert!(policy.contains("connect-src 'self' https://files.example http://files.example"));
    assert!(!policy.contains("not a host"));
    assert!(!policy.contains("example.com "));
    assert!(policy.contains("script-src 'self'"));
    assert!(policy.contains("default-src 'none'"));
    assert!(!allows_navigation(PLUGIN, "https://files.example/draw"));
}

#[test]
fn mime_types_follow_the_extension() {
    assert_eq!(mime("ui/index.html"), "text/html; charset=utf-8");
    assert_eq!(mime("ui/INDEX.HTML"), "text/html; charset=utf-8");
    assert_eq!(mime("app.css"), "text/css; charset=utf-8");
    assert_eq!(mime("APP.CSS"), "text/css; charset=utf-8");
    assert_eq!(mime("app.js"), "text/javascript; charset=utf-8");
    assert_eq!(mime("APP.JS"), "text/javascript; charset=utf-8");
    assert_eq!(mime("icon.svg"), "image/svg+xml");
    assert_eq!(mime("pic.png"), "image/png");
    assert_eq!(mime("no-extension"), "application/octet-stream");
}

#[test]
fn the_page_is_served_from_its_own_folder() {
    let root = tempfile::tempdir().unwrap();
    let ui = root.path().join("ui");
    std::fs::create_dir(&ui).unwrap();
    std::fs::write(ui.join("index.html"), b"<p>hello</p>").unwrap();
    std::fs::write(ui.join("app.js"), b"kagan.post(1)").unwrap();
    let page = serve(
        root.path(),
        PLUGIN,
        "GET",
        "kagan-plugin://dev.kagan.page/ui/index.html",
        &[],
        ":root{}",
    );
    assert_eq!(page.status, 200);
    assert_eq!(page.mime, "text/html; charset=utf-8");
    assert_eq!(page.body, b"<p>hello</p>");
    assert!(page.csp.contains("default-src 'none'"));

    let script = serve(
        root.path(),
        PLUGIN,
        "GET",
        "kagan-plugin://dev.kagan.page/ui/app.js",
        &[],
        "",
    );
    assert_eq!(script.mime, "text/javascript; charset=utf-8");

    let theme = serve(
        root.path(),
        PLUGIN,
        "GET",
        "kagan-plugin://dev.kagan.page/__kagan/theme.css",
        &[],
        ":root{--bg:#fff}",
    );
    assert_eq!(theme.status, 200);
    assert_eq!(theme.body, b":root{--bg:#fff}");
    assert_eq!(theme.mime, "text/css; charset=utf-8");
    let theme_case = serve(
        root.path(),
        PLUGIN,
        "GET",
        "kagan-plugin://dev.kagan.page/__KAGAN/theme.css",
        &[],
        ":root{--bg:#fff}",
    );
    assert_eq!(theme_case.status, 200);
    assert_eq!(theme_case.body, b":root{--bg:#fff}");

    assert_eq!(
        serve(
            root.path(),
            PLUGIN,
            "GET",
            "https://example.com/ui/index.html",
            &[],
            ""
        )
        .status,
        403
    );
    assert_eq!(
        serve(
            root.path(),
            PLUGIN,
            "POST",
            "kagan-plugin://dev.kagan.page/ui/index.html",
            &[],
            ""
        )
        .status,
        405
    );
    assert_eq!(
        serve(
            root.path(),
            PLUGIN,
            "GET",
            "kagan-plugin://dev.kagan.page/missing.html",
            &[],
            ""
        )
        .status,
        404
    );
    let _ = Path::new("ui");
}

#[cfg(unix)]
#[test]
fn a_symlink_out_is_not_served() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("secret.html");
    std::fs::write(&outside, b"secret-outside").unwrap();
    let plugin = root.path().join("plugin");
    std::fs::create_dir(&plugin).unwrap();
    std::os::unix::fs::symlink(&outside, plugin.join("index.html")).unwrap();
    let served = serve(
        &plugin,
        PLUGIN,
        "GET",
        "kagan-plugin://dev.kagan.page/index.html",
        &[],
        "",
    );
    assert_eq!(served.status, 403);
    assert!(!String::from_utf8_lossy(&served.body).contains("secret-outside"));
}

#[test]
fn a_page_event_is_json() {
    let script = page_event(r#"{"from":"plugin"}"#).unwrap();
    assert!(script.contains("JSON.parse"));
    assert!(script.contains(r#"{\"from\":\"plugin\"}"#));
    assert!(page_event("\u{2028}").is_some());
}

#[test]
fn page_paths_round_trip_through_the_url() {
    let root = tempfile::tempdir().unwrap();
    let ui = root.path().join("ui");
    std::fs::create_dir(&ui).unwrap();
    std::fs::write(ui.join("my page.html"), b"<p>space</p>").unwrap();
    std::fs::write(ui.join("café.html"), b"<p>unicode</p>").unwrap();
    std::fs::write(ui.join("100%.html"), b"<p>percent</p>").unwrap();
    std::fs::write(ui.join("a?b.html"), b"<p>query</p>").unwrap();
    std::fs::write(ui.join("a#b.html"), b"<p>hash</p>").unwrap();

    for (file, body) in [
        ("ui/my page.html", b"<p>space</p>".as_slice()),
        ("ui/café.html", b"<p>unicode</p>"),
        ("ui/100%.html", b"<p>percent</p>"),
        ("ui/a?b.html", b"<p>query</p>"),
        ("ui/a#b.html", b"<p>hash</p>"),
    ] {
        let url = page_url(PLUGIN, file).unwrap();
        assert!(!url.contains(' '), "{url}");
        assert!(!url.contains('#'), "{url}");
        let served = serve(root.path(), PLUGIN, "GET", &url, &[], "");
        assert_eq!(served.status, 200, "{file} -> {url}");
        assert_eq!(served.body, body, "{file}");
    }

    let traversal = format!(
        "kagan-plugin://{PLUGIN}/{}",
        percent_encoding::utf8_percent_encode("../secret.html", percent_encoding::NON_ALPHANUMERIC)
    );
    assert_eq!(
        serve(root.path(), PLUGIN, "GET", &traversal, &[], "").status,
        403
    );
    let encoded_dot = format!("kagan-plugin://{PLUGIN}/%2e%2e/secret.html");
    assert_eq!(
        serve(root.path(), PLUGIN, "GET", &encoded_dot, &[], "").status,
        403
    );
    assert_eq!(
        serve(
            root.path(),
            PLUGIN,
            "GET",
            &format!("kagan-plugin://{PLUGIN}/ui/%zz.html"),
            &[],
            ""
        )
        .status,
        403
    );
}
