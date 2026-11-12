use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use serde_json::json;

use super::{click, open, present, set_preference, wait_until};

/// Nothing listens on port 1, so the usage request is refused at once and reaches no provider.
const REFUSED: &str = "http://127.0.0.1:1/wham/usage";

fn sign_in(dir: &Path) -> PathBuf {
    let home = dir.join("codex");
    std::fs::create_dir_all(&home).unwrap();
    let auth = json!({ "tokens": { "access_token": "fixture-token" } });
    std::fs::write(home.join("auth.json"), auth.to_string()).unwrap();
    home
}

fn point_at(cx: &mut TestAppContext, home: PathBuf) {
    cx.update(|cx| kg_core::use_codex_source(cx, Some(home), REFUSED.to_owned()));
}

#[gpui_kit::test]
fn the_usage_button_follows_the_preference(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    wait_until(cx, |cx| present(cx, window, "usage"));
    set_preference(cx, "usage-limits", json!(false));
    wait_until(cx, |cx| !present(cx, window, "usage"));
}

#[gpui_kit::test]
fn the_popup_says_when_no_provider_is_signed_in(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    point_at(cx, data.path().join("no-codex"));
    set_preference(cx, "usage-limits", json!(true));
    wait_until(cx, |cx| present(cx, window, "usage"));
    click(cx, window, "usage");
    wait_until(cx, |cx| present(cx, window, "usage-panel"));
    assert!(present(cx, window, "usage-refresh"));
    assert!(!present(cx, window, "usage-retry"));
}

#[gpui_kit::test]
fn a_reading_that_fails_offers_a_retry(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    point_at(cx, sign_in(data.path()));
    set_preference(cx, "usage-limits", json!(true));
    wait_until(cx, |cx| present(cx, window, "usage"));
    click(cx, window, "usage");
    wait_until(cx, |cx| present(cx, window, "usage-retry"));
}

#[cfg(unix)]
#[gpui_kit::test]
fn claude_is_started_only_when_the_popup_opens(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt as _;

    let (_kagan, window, data) = open(cx);
    let folder = data.path().join("claude-bin");
    std::fs::create_dir_all(&folder).unwrap();
    let marker = data.path().join("started");
    let program = folder.join("claude");
    let script = format!("#!/bin/sh\ntouch '{}'\ncat > /dev/null\n", marker.display());
    std::fs::write(&program, script).unwrap();
    std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
    cx.update(|cx| kg_core::use_claude_path(cx, Some(folder.into_os_string())));
    wait_until(cx, |cx| present(cx, window, "usage"));
    assert!(!marker.exists(), "starting kagan never starts Claude");

    click(cx, window, "usage");
    wait_until(cx, |_| marker.exists());
}
