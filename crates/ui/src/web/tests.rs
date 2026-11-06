use super::{allows_navigation, csp, mime, page_event, page_url, refresh, serve, stylesheet};
#[cfg(target_os = "macos")]
use crate::web_page::durable_store;

const PLUGIN: &str = "dev.kagan.page";

#[test]
fn the_stylesheet_and_the_refresh_script_share_every_token() {
    let tokens = [
        ("--bg", "#010203".to_owned()),
        ("--ink", "#aabbcc".to_owned()),
    ];
    let css = stylesheet(&tokens, "dark");
    let script = refresh(&tokens, "dark");
    assert!(css.starts_with(":root{--bg:#010203;--ink:#aabbcc;color-scheme:dark}"));
    for (name, value) in &tokens {
        assert!(script.contains(&format!("'{name}':'{value}'")));
    }
    assert!(script.contains("r.style.colorScheme='dark'"));
}

#[test]
fn the_page_gets_the_themes_density_and_type() {
    let json = r##"{
        "name":"Tight",
        "light":{
            "bg":"#ffffff","surface":"#eeeeee","raised":"#dddddd","ink":"#111111",
            "muted":"#555555","line":"#cccccc","accent":"#aa0000",
            "primaryForeground":"#ffffff","working":"#0000aa","needs":"#aa8800",
            "failed":"#aa0000","idle":"#777777","selection":"#112233","input":"#445566"
        },
        "dark":{
            "bg":"#111111","surface":"#222222","raised":"#333333","ink":"#eeeeee",
            "muted":"#999999","line":"#444444","accent":"#ff8800",
            "primaryForeground":"#111111","working":"#7799ff","needs":"#e8cf80",
            "failed":"#ffa1ad","idle":"#999999"
        },
        "look":{
            "space":{"xs":2,"sm":6,"md":10,"lg":14,"xl":20,"xxl":28},
            "radius":{"xs":0,"sm":0,"md":2,"lg":4},
            "type":{"caption":10,"small":11,"body":12,"title":14,"mono":11},
            "row":24,"control":20
        }
    }"##;
    let file = kg_domain::ThemeFile::parse(json.as_bytes()).unwrap();
    let css = stylesheet(&super::tokens(&file.light, &file.look), "light");
    for want in [
        "--space-xs:2px",
        "--space-sm:6px",
        "--space-md:10px",
        "--space-lg:14px",
        "--space-xl:20px",
        "--space-xxl:28px",
        "--radius-xs:0px",
        "--radius-sm:0px",
        "--radius-md:2px",
        "--radius-lg:4px",
        "--text-caption:10px",
        "--text-small:11px",
        "--text-body:12px",
        "--text-title:14px",
        "--text-mono:11px",
        "--row:24px",
        "--control:20px",
        "--selection:#112233",
        "--input:#445566",
        "--bg:#ffffff",
    ] {
        assert!(css.contains(want), "{want} missing from {css}");
    }
}

#[test]
fn a_default_look_keeps_the_numbers_the_page_had_before() {
    let file = kg_domain::ThemeFile::parse(
        br##"{"name":"Tight","light":{
            "bg":"#ffffff","surface":"#eeeeee","raised":"#dddddd","ink":"#111111",
            "muted":"#555555","line":"#cccccc","accent":"#aa0000",
            "primaryForeground":"#ffffff","working":"#0000aa","needs":"#aa8800",
            "failed":"#aa0000","idle":"#777777"
        },"dark":{
            "bg":"#111111","surface":"#222222","raised":"#333333","ink":"#eeeeee",
            "muted":"#999999","line":"#444444","accent":"#ff8800",
            "primaryForeground":"#111111","working":"#7799ff","needs":"#e8cf80",
            "failed":"#ffa1ad","idle":"#999999"
        }}"##,
    )
    .unwrap();
    let css = stylesheet(&super::tokens(&file.light, &file.look), "light");
    for want in [
        "--space-xs:4px",
        "--space-sm:8px",
        "--space-md:12px",
        "--space-lg:16px",
        "--space-xl:24px",
        "--space-xxl:32px",
        "--radius-xs:6px",
        "--radius-sm:8px",
        "--radius-md:8px",
        "--radius-lg:12px",
        "--text-caption:11px",
        "--text-small:12px",
        "--text-body:13px",
        "--text-title:15px",
        "--text-mono:12px",
        "--row:28px",
        "--control:24px",
        "--selection:#dddddd",
        "--input:#cccccc",
    ] {
        assert!(css.contains(want), "{want} missing from {css}");
    }
}

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
fn a_windows_page_keeps_the_plugin_origin_through_webview2() {
    // WebView2 rewrites the page's own origin, and navigation reports the rewritten form.
    let page = "http://kagan-plugin.dev.kagan.page/ui/index.html";
    assert!(allows_navigation(PLUGIN, page));
    assert!(allows_navigation(
        PLUGIN,
        "http://kagan-plugin.dev.kagan.page/a?b#c"
    ));
    assert!(!allows_navigation(
        PLUGIN,
        "http://kagan-plugin.dev.other.page/ui/index.html"
    ));
    assert!(!allows_navigation(
        PLUGIN,
        "http://kagan-plugin.dev.kagan.page:8080/x"
    ));
    assert!(!allows_navigation(PLUGIN, "http://example.com/"));
    assert!(!allows_navigation(
        PLUGIN,
        "http://kagan-plugin.dev.kagan.page@evil.com/"
    ));
    let own = page_url(PLUGIN, "ui/index.html").unwrap();
    assert!(allows_navigation(PLUGIN, &own));
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
    // Windows cannot create `?` in a name; `a_query_and_a_hash_stay_in_the_page_path` covers it.
    std::fs::write(ui.join("a#b.html"), b"<p>hash</p>").unwrap();

    for (file, body) in [
        ("ui/my page.html", b"<p>space</p>".as_slice()),
        ("ui/café.html", b"<p>unicode</p>"),
        ("ui/100%.html", b"<p>percent</p>"),
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

#[test]
fn a_query_and_a_hash_stay_in_the_page_path() {
    for (file, encoded) in [
        ("ui/a?b.html", "ui/a%3Fb.html"),
        ("ui/a#b.html", "ui/a%23b.html"),
    ] {
        let url = page_url(PLUGIN, file).unwrap();
        assert!(url.ends_with(encoded), "{url}");
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_store_id_is_only_durable_on_macos_14() {
    let id = [1u8; 16];
    assert_eq!(durable_store(Some(id), 13), None);
    assert_eq!(durable_store(Some(id), 14), Some(id));
    assert_eq!(durable_store(None, 14), None);
}
