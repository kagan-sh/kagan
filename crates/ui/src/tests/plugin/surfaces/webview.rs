use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{Op, PluginId, PluginOp};
use serde_json::json;

use super::{
    body, box_of, close_surface, install_pair, item, manifest, open_surface, run, surfaces, wide,
    write_plugin,
};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, present, selected_of, shows, wait_until,
};

const ALPHA: &str = "dev.test.alpha";

#[gpui_kit::test]
fn an_open_draws_a_right_panel_tab(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    let _asked =
        cx.update(|cx| Workspace::ask_for_person(None, None, "Waiting?".into(), vec![], cx));
    cx.run_until_parked();
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(1120.), px(720.)));
    cx.run_until_parked();
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    assert!(item(&surfaces(cx), ALPHA, "side").visible);
    assert!(shows(cx, window, "right-tab-bar"));
    assert!(shows(cx, window, "right-tab-add"));
    assert!(shows(cx, window, &format!("right-tab-panel-{ALPHA}-side")));
    assert!(shows(cx, window, "needs-you-header"));
    assert!(!present(cx, window, "right-tab-close-needs-you"));

    click(cx, window, &format!("right-tab-close-panel-{ALPHA}-side"));
    wait_until(cx, |cx| !shows(cx, window, &body(ALPHA, "side")));
    assert!(!item(&surfaces(cx), ALPHA, "side").open);
    assert_eq!(focused_of(cx, window, "right-tab-add"), Some(true));

    click(cx, window, "right-tab-add");
    click(cx, window, &format!("right-add-{ALPHA}-side"));
    wait_until(cx, |cx| shows(cx, window, &body(ALPHA, "side")));
    assert_eq!(
        selected_of(cx, window, &format!("right-tab-panel-{ALPHA}-side")),
        Some(true)
    );
}

pub(in crate::tests) const WEB: &str = "dev.test.webview";

pub(in crate::tests) const WEB_SOURCE: &str = r#"// @ts-check
import { View } from "gpui-kit";
import { WebView } from "gpui-component";
export default class Plugin extends View {
  render() { return ""; }
}
export class Page extends View {
  render() {
    return new WebView("editor").url("http://127.0.0.1:1/");
  }
}
export class Fetch extends View {
  render() {
    return new WebView("fetch").url("https://files.example/draw");
  }
}
export class Off extends View {
  render() {
    return new WebView("off").url("https://example.com/");
  }
}
"#;

pub(in crate::tests) fn install_webview(data: &Path) {
    let mut spec = manifest(
        WEB,
        "Web view",
        json!([
            {"id": "page", "title": "Editor", "view": "Page", "placement": "right"},
            {"id": "fetch", "title": "Fetch", "view": "Fetch", "placement": "right"},
            {"id": "off", "title": "Off", "view": "Off", "placement": "right"}
        ]),
        false,
    );
    spec["powers"]["fetch"] = json!([{ "host": "files.example", "methods": ["GET"] }]);
    write_plugin(data, "webview", WEB, &spec, WEB_SOURCE);
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .set_plugin_approved(
            &PluginId(WEB.into()),
            &json!({
                "powers": { "fetch": [{ "host": "files.example", "methods": ["GET"] }] }
            }),
        )
        .unwrap();
}

pub(in crate::tests) fn placed(native: Option<bool>, note: &str) -> bool {
    native == Some(true)
        || note.contains("could not open")
        || note.contains("WebView2 Runtime")
        || platform_note(note)
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
pub(in crate::tests) const fn platform_note(_note: &str) -> bool {
    false
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub(in crate::tests) fn platform_note(note: &str) -> bool {
    note == crate::components::UNAVAILABLE
}

pub(in crate::tests) fn page_place(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    id: &str,
) -> (Option<bool>, String) {
    let note = label(cx, window, &format!("webview-{id}-note"));
    let native = cx
        .update_window(window, |_, _, cx| crate::components::native_shown(cx, id))
        .unwrap();
    (native, note)
}

#[gpui_kit::test]
fn a_plugin_right_panel_mounts_a_web_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_webview);
    wide(cx, window);
    open_surface(cx, WEB, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "webview-editor"));
    cx.run_until_parked();
    let page = box_of(cx, window, "webview-editor");
    let side = box_of(cx, window, "right-sidebar");
    let panel = box_of(cx, window, &body(WEB, "page"));
    assert!(
        (page.0 - panel.0).abs() < 1.
            && (page.1 - panel.1).abs() < 1.
            && (page.2 - panel.2).abs() < 1.
            && (page.3 - panel.3).abs() < 1.,
        "page {page:?} panel {panel:?}"
    );
    assert!(
        (page.0 - side.0).abs() < 1. && (page.2 - side.2).abs() < 1.,
        "page {page:?} side {side:?}"
    );
    assert!(page.1 > side.1, "page {page:?} side {side:?}");
    assert!(
        ((page.1 + page.3) - (side.1 + side.3)).abs() < 1.,
        "page {page:?} side {side:?}"
    );
    let (native, note) = page_place(cx, window, "editor");
    assert!(
        placed(native, &note),
        "a 127.0.0.1 page did not place: native {native:?} note {note:?}"
    );

    open_surface(cx, WEB, "fetch").unwrap();
    wait_until(cx, |cx| shows(cx, window, "webview-fetch"));
    cx.run_until_parked();
    let (native, note) = page_place(cx, window, "fetch");
    assert!(
        placed(native, &note),
        "a powers.fetch host did not place: native {native:?} note {note:?}"
    );

    open_surface(cx, WEB, "off").unwrap();
    wait_until(cx, |cx| shows(cx, window, "webview-off-note"));
    assert!(!shows(cx, window, "webview-editor"));
    assert_eq!(
        label(cx, window, "webview-off-note"),
        crate::components::REFUSED_HOST
    );
    let hidden = cx
        .update_window(window, |_, _, cx| {
            crate::components::native_shown(cx, "editor")
        })
        .unwrap();
    assert_ne!(hidden, Some(true));

    click(cx, window, &format!("right-tab-panel-{WEB}-page"));
    wait_until(cx, |cx| shows(cx, window, "webview-editor"));
    cx.run_until_parked();
    let back = cx
        .update_window(window, |_, _, cx| {
            crate::components::native_shown(cx, "editor")
        })
        .unwrap();
    if back.is_some() {
        assert_eq!(back, Some(true));
    }
}

pub(in crate::tests) const WEB_PANEL: &str = "dev.test.web-panel";

pub(in crate::tests) const WEB_PANEL_SOURCE: &str = r#"import { View } from "gpui-kit";
export default class Plugin extends View {
  render() { return ""; }
}
"#;

pub(in crate::tests) fn install_web_panel(data: &Path) {
    write_plugin(
        data,
        "web-panel",
        WEB_PANEL,
        &manifest(
            WEB_PANEL,
            "Web panel",
            json!([
                {"id": "board", "title": "Board", "web": "ui/index.html", "placement": "right"},
                {"id": "stage", "title": "Stage", "web": "ui/index.html", "placement": "center"}
            ]),
            false,
        ),
        WEB_PANEL_SOURCE,
    );
    let ui = data.join("web-panel").join("ui");
    std::fs::create_dir(&ui).unwrap();
    std::fs::write(ui.join("index.html"), "<p>page</p>").unwrap();
}

pub(in crate::tests) fn web_panel_id(plugin: &str, id: &str) -> String {
    format!("web-panel-panel-{plugin}-{id}")
}

#[gpui_kit::test]
fn a_web_panel_is_labeled_for_kg_ui(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_web_panel);
    wide(cx, window);
    open_surface(cx, WEB_PANEL, "board").unwrap();
    let right = web_panel_id(WEB_PANEL, "board");
    wait_until(cx, |cx| shows(cx, window, &right));
    assert_eq!(label(cx, window, &right), "Web panel");

    open_surface(cx, WEB_PANEL, "stage").unwrap();
    let center = web_panel_id(WEB_PANEL, "stage");
    wait_until(cx, |cx| shows(cx, window, &center));
    assert_eq!(label(cx, window, &center), "Web panel");
}

#[cfg(target_os = "macos")]
pub(in crate::tests) const LIFE: &str = "dev.test.web-life";
#[cfg(target_os = "macos")]
pub(in crate::tests) const HIDE: &str = "dev.test.web-hide";

#[cfg(target_os = "macos")]
pub(in crate::tests) fn life_script(ready: &Path, seen: &Path) -> String {
    let ready = serde_json::to_string(&ready.display().to_string()).unwrap();
    let seen = serde_json::to_string(&seen.display().to_string()).unwrap();
    format!(
        r#"import {{ View }} from "gpui-kit";
import {{ next, reply, fail }} from "kagan/jobs";
import {{ writeFile }} from "fs/promises";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      await writeFile({ready}, "ready");
      for (;;) {{
        const job = await next();
        try {{
          if (job.kind === "page") await writeFile({seen}, JSON.stringify(job.message));
          reply(job.id, null);
        }} catch (error) {{
          fail(job.id, String(error));
        }}
      }}
    }});
  }}
  render() {{ return ""; }}
}}
"#
    )
}

#[cfg(target_os = "macos")]
pub(in crate::tests) fn life_seen(data: &Path) -> std::path::PathBuf {
    data.join("plugins")
        .join(LIFE)
        .join("data")
        .join("seen.json")
}

#[cfg(target_os = "macos")]
pub(in crate::tests) fn install_life(data: &Path) {
    let dir = data.join("plugins").join(LIFE).join("data");
    std::fs::create_dir_all(&dir).unwrap();
    write_plugin(
        data,
        "web-life",
        LIFE,
        &manifest(
            LIFE,
            "Web life",
            json!([
                {"id": "board", "title": "Board", "web": "ui/index.html", "placement": "right"}
            ]),
            false,
        ),
        &life_script(&dir.join("ready"), &dir.join("seen.json")),
    );
    let ui = data.join("web-life").join("ui");
    std::fs::create_dir(&ui).unwrap();
    std::fs::write(ui.join("index.html"), "<p>page</p>").unwrap();
}

#[cfg(target_os = "macos")]
pub(in crate::tests) fn open_life(
    cx: &mut TestAppContext,
) -> (
    gpui_kit::Entity<crate::kagan::Kagan>,
    AnyWindowHandle,
    tempfile::TempDir,
    std::path::PathBuf,
) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_life);
    wide(cx, window);
    cx.update(|cx| {
        kg_plugins::PluginHost::global(cx)
            .update(cx, |host, cx| host.load_plugin(&PluginId(LIFE.into()), cx))
            .unwrap();
    });
    let seen = life_seen(data.path());
    let ready = seen.parent().unwrap().join("ready");
    wait_until(cx, |_| ready.is_file());
    let _ = std::fs::remove_file(&seen);
    (kagan, window, data, seen)
}

#[cfg(target_os = "macos")]
pub(in crate::tests) fn missed(seen: &Path, mark: &str) {
    let text = std::fs::read_to_string(seen).unwrap_or_default();
    assert!(!text.contains(mark), "{text}");
}

#[cfg(target_os = "macos")]
pub(in crate::tests) fn life_slot() -> String {
    format!("panel-{LIFE}-board")
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_closed_web_panel_gets_no_page_job(cx: &mut TestAppContext) {
    let (_kagan, _window, _data, seen) = open_life(cx);
    let plugin = PluginId(LIFE.into());
    cx.update(|cx| {
        crate::kagan::testing_page(&plugin, "board", &life_slot(), true, cx);
        assert!(crate::kagan::inject(
            &plugin,
            "board",
            r#"{"at":"close"}"#,
            cx
        ));
        crate::kagan::forget(&life_slot(), cx);
    });
    open_surface(cx, LIFE, "board").unwrap();
    close_surface(cx, LIFE, "board").unwrap();
    missed(&seen, "close");
    let again = cx.update(|cx| crate::kagan::inject(&plugin, "board", r#"{"at":"after"}"#, cx));
    assert!(!again);
    cx.run_until_parked();
    missed(&seen, "after");
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_reloaded_web_panel_gets_no_queued_page_job(cx: &mut TestAppContext) {
    let (_kagan, _window, _data, seen) = open_life(cx);
    cx.update(|cx| {
        let plugin = PluginId(LIFE.into());
        crate::kagan::testing_page(&plugin, "board", &life_slot(), true, cx);
        assert!(crate::kagan::inject(
            &plugin,
            "board",
            r#"{"at":"reload"}"#,
            cx
        ));
        assert!(crate::kagan::mark_stale(&life_slot(), cx));
    });
    cx.run_until_parked();
    missed(&seen, "reload");
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(LIFE.into()),
            thread_id: None,
        }),
    )
    .unwrap();
    cx.run_until_parked();
    missed(&seen, "reload");
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_closed_window_gets_no_page_job(cx: &mut TestAppContext) {
    let (kagan, window, _data, seen) = open_life(cx);
    cx.update(|cx| {
        let plugin = PluginId(LIFE.into());
        crate::kagan::testing_page(&plugin, "board", &life_slot(), true, cx);
        assert!(crate::kagan::inject(
            &plugin,
            "board",
            r#"{"at":"window"}"#,
            cx
        ));
    });
    drop(kagan);
    cx.update_window(window, |_, window, _| window.remove_window())
        .unwrap();
    cx.run_until_parked();
    missed(&seen, "window");
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_page_job_queued_before_close_does_not_reach_the_reopened_panel(cx: &mut TestAppContext) {
    let (_kagan, _window, _data, seen) = open_life(cx);
    let plugin = PluginId(LIFE.into());
    cx.update(|cx| {
        crate::kagan::testing_page(&plugin, "board", &life_slot(), true, cx);
        assert!(crate::kagan::queue_page(
            &plugin,
            "board",
            r#"{"at":"stale"}"#,
            cx
        ));
        crate::kagan::forget(&life_slot(), cx);
    });
    cx.update(|cx| {
        crate::kagan::testing_page(&plugin, "board", &life_slot(), true, cx);
        assert!(crate::kagan::queue_page(
            &plugin,
            "board",
            r#"{"at":"fresh"}"#,
            cx
        ));
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    loop {
        cx.run_until_parked();
        if std::fs::read_to_string(&seen)
            .unwrap_or_default()
            .contains("fresh")
        {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the page job"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    let text = std::fs::read_to_string(&seen).unwrap();
    assert!(!text.contains("stale"), "{text}");
    assert!(text.contains("fresh"), "{text}");
}

#[cfg(target_os = "macos")]
#[gpui_kit::test]
fn a_hidden_web_panel_refuses_post(cx: &mut TestAppContext) {
    let (_kagan, _window, _data) = open_with_plugins_then(cx, |_| {});
    let plugin = PluginId(HIDE.into());
    cx.update(|cx| {
        crate::kagan::testing_page(&plugin, "board", "board", false, cx);
        crate::kagan::testing_page(&plugin, "notes", "notes", true, cx);
    });
    let hidden = cx.update(|cx| crate::kagan::post(&plugin, "board", r#"{"hi":1}"#, cx));
    let error = hidden.unwrap_err();
    assert!(error.contains("not on screen"), "{error}");
    let shown = cx.update(|cx| crate::kagan::post(&plugin, "notes", r#"{"hi":1}"#, cx));
    let error = shown.unwrap_err();
    assert!(!error.contains("not on screen"), "{error}");
}
