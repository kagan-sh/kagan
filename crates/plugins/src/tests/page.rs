use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_domain::PluginId;

use crate::page::{PageFileError, parse_message, read_plugin_file};

use super::{
    PAGE_PLUGIN, deliver_page, flood_page, open_page, open_page_holding_next, pending_page_bytes,
};

#[test]
fn a_file_inside_the_plugin_is_read() {
    let root = tempfile::tempdir().unwrap();
    let page = root.path().join("ui");
    std::fs::create_dir(&page).unwrap();
    std::fs::write(page.join("index.html"), b"<p>hello</p>").unwrap();
    let bytes = read_plugin_file(root.path(), "ui/index.html").unwrap();
    assert_eq!(bytes, b"<p>hello</p>");
}

#[test]
fn a_path_that_leaves_the_plugin_is_refused() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("index.html"), b"inside").unwrap();
    for path in [
        "../index.html",
        "/etc/passwd",
        "ui/../../index.html",
        "..",
        r"ui\index.html",
    ] {
        assert_eq!(
            read_plugin_file(root.path(), path),
            Err(PageFileError::Refused),
            "{path}"
        );
    }
}

#[test]
fn a_missing_file_is_missing() {
    let root = tempfile::tempdir().unwrap();
    assert_eq!(
        read_plugin_file(root.path(), "ui/index.html"),
        Err(PageFileError::Missing)
    );
}

#[cfg(unix)]
#[test]
fn a_symlink_out_of_the_plugin_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside.html");
    std::fs::write(&outside, b"secret-outside").unwrap();
    let plugin = root.path().join("plugin");
    std::fs::create_dir(&plugin).unwrap();
    std::os::unix::fs::symlink(&outside, plugin.join("index.html")).unwrap();
    let read = read_plugin_file(&plugin, "index.html");
    assert_eq!(read, Err(PageFileError::Refused));
    let shown = match &read {
        Ok(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Err(error) => error.to_string(),
    };
    assert!(!shown.contains("secret-outside"), "{shown}");
}

#[test]
fn a_page_file_error_is_an_error() {
    let error = PageFileError::TooBig;
    let reported: &dyn std::error::Error = &error;
    assert!(reported.to_string().contains("8 MiB"));
}

#[test]
fn a_file_over_the_cap_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("big.html");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(8 * 1024 * 1024 + 1).unwrap();
    assert_eq!(
        read_plugin_file(root.path(), "big.html"),
        Err(PageFileError::TooBig)
    );
}

#[test]
fn a_file_at_the_cap_is_read() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("edge.html");
    let file = std::fs::File::create(&path).unwrap();
    file.set_len(8 * 1024 * 1024).unwrap();
    assert!(read_plugin_file(root.path(), "edge.html").is_ok());
}

#[test]
fn a_page_that_is_missing_fails_the_check() {
    let root = tempfile::tempdir().unwrap();
    assert!(crate::page::require_html(root.path(), "index.html").is_err());
    std::fs::write(root.path().join("index.html"), "<p>hi</p>").unwrap();
    assert!(crate::page::require_html(root.path(), "index.html").is_ok());
}

#[test]
fn a_page_message_is_json_within_the_cap() {
    assert_eq!(parse_message(r#"{"ok":true}"#).unwrap()["ok"], true);
    assert!(parse_message("not json").is_err());
    let at = format!("\"{}\"", "x".repeat(1024 * 1024 - 2));
    assert!(parse_message(&at).is_ok());
    let huge = format!("\"{}\"", "x".repeat(1024 * 1024));
    assert!(
        parse_message(&huge)
            .unwrap_err()
            .to_string()
            .contains("over the")
    );
}

#[test]
fn the_reserved_folder_is_not_a_plugin_file() {
    let root = tempfile::tempdir().unwrap();
    let reserved = root.path().join("__kagan");
    std::fs::create_dir(&reserved).unwrap();
    std::fs::write(reserved.join("theme.css"), b"body{}").unwrap();
    assert_eq!(
        read_plugin_file(root.path(), "__kagan/theme.css"),
        Err(PageFileError::Refused)
    );
    assert_eq!(
        read_plugin_file(root.path(), "__KAGAN/theme.css"),
        Err(PageFileError::Refused)
    );
}

#[gpui_kit::test]
fn a_page_message_round_trips_through_the_plugin(cx: &mut TestAppContext) {
    let run = open_page(cx);
    deliver_page(cx, &run, r#"{"from":"page"}"#).unwrap();
    let mut echoed = String::new();
    for _ in 0..2000 {
        cx.run_until_parked();
        if echoed.is_empty()
            && let Ok(message) = run.posted.try_recv()
        {
            echoed = message;
        }
        if run.seen.is_file() && !echoed.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(
        std::fs::read_to_string(&run.seen).unwrap(),
        r#"{"from":"page"}"#
    );
    assert!(
        echoed.contains("\"echo\"") && echoed.contains("\"from\":\"page\""),
        "{echoed}"
    );
}

#[gpui_kit::test]
fn a_page_message_over_the_cap_is_refused(cx: &mut TestAppContext) {
    let run = open_page(cx);
    let over = deliver_page(cx, &run, &format!("\"{}\"", "y".repeat(crate::MESSAGE_CAP)));
    assert!(over.unwrap_err().to_string().contains("over the"));
    deliver_page(cx, &run, r#"{"big":true}"#).unwrap();
    for _ in 0..2000 {
        cx.run_until_parked();
        if run.seen.is_file() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let error = std::fs::read_to_string(&run.seen).unwrap();
    assert!(error.starts_with("error:"), "{error}");
    assert!(error.contains("over the"), "{error}");
}

#[gpui_kit::test]
fn a_page_flood_while_next_is_withheld_is_bounded(cx: &mut TestAppContext) {
    let _plugin = open_page_holding_next(cx);
    let life = std::sync::Arc::new(());
    let mut accepted = 0;
    for i in 0..crate::jobs::PAGE_PENDING_CAP + 8 {
        let raw = format!(r#"{{"n":{i}}}"#);
        match flood_page(cx, &life, &raw) {
            Ok(()) => accepted += 1,
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("cannot take another page message"),
                    "{error}"
                );
                break;
            }
        }
    }
    assert_eq!(accepted, crate::jobs::PAGE_PENDING_CAP);
    assert!(
        pending_page_bytes(cx) <= crate::jobs::PAGE_PENDING_BYTES,
        "{}",
        pending_page_bytes(cx)
    );
    assert!(
        flood_page(cx, &life, r#"{"n":"over"}"#)
            .unwrap_err()
            .to_string()
            .contains("cannot take another page message")
    );

    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(pending_page_bytes(cx), 0);
    let mut after = 0;
    for i in 0..crate::jobs::PAGE_PENDING_CAP + 8 {
        let raw = format!(r#"{{"again":{i}}}"#);
        match flood_page(cx, &life, &raw) {
            Ok(()) => after += 1,
            Err(error) => {
                assert!(
                    error
                        .to_string()
                        .contains("cannot take another page message"),
                    "{error}"
                );
                break;
            }
        }
    }
    assert_eq!(after, crate::jobs::PAGE_PENDING_CAP);
    assert!(
        pending_page_bytes(cx) <= crate::jobs::PAGE_PENDING_BYTES,
        "{}",
        pending_page_bytes(cx)
    );
}

#[gpui_kit::test]
fn a_page_job_expires_when_its_page_token_dies(cx: &mut TestAppContext) {
    let run = open_page(cx);
    cx.update(|cx| {
        let stale_life = std::sync::Arc::new(());
        let alive = std::sync::Arc::downgrade(&stale_life);
        crate::PluginHost::global(cx)
            .update(cx, |host, cx| {
                host.deliver_page(
                    &PluginId(PAGE_PLUGIN.into()),
                    "board",
                    r#"{"at":"stale"}"#,
                    alive,
                    cx,
                )
            })
            .unwrap();
        drop(stale_life);
    });
    deliver_page(cx, &run, r#"{"at":"fresh"}"#).unwrap();
    for _ in 0..2000 {
        cx.run_until_parked();
        if std::fs::read_to_string(&run.seen)
            .unwrap_or_default()
            .contains("fresh")
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let text = std::fs::read_to_string(&run.seen).unwrap();
    assert!(!text.contains("stale"), "{text}");
    assert!(text.contains("fresh"), "{text}");
}
