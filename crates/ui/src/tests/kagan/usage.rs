use std::path::{Path, PathBuf};

use gpui_kit::TestAppContext;
use serde_json::json;

use super::{click, open, present, set_preference, wait_until};

/// Nothing listens on port 1, so the usage request is refused at once and reaches no provider.
const REFUSED: &str = "https://127.0.0.1:1/wham/usage";

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
    assert!(!present(cx, window, "usage"));
    set_preference(cx, "usage-limits", json!(true));
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
