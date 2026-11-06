//! A plugin with the browser area opens the built-in Browser tab and adds no panel of its own.

use std::path::Path;
use std::time::{Duration, Instant};

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use std::io::Write as _;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use gpui_kit::AppContext as _;
use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, Plugin, PluginId, ThreadOp};
use serde_json::json;

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use crate::tests::kagan::selected_of;
use crate::tests::kagan::{click, open_with_plugins_then, present, run_shell, shows};
#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
use crate::tests::plugin::panels::disabled_of;

const DRIVE: &str = "dev.test.drive";

fn install_drive(data: &Path) {
    let root = data.join("drive");
    std::fs::create_dir(&root).unwrap();
    let manifest = json!({
        "id": DRIVE,
        "name": "Drive",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "Open the Browser tab.",
        "powers": { "areas": ["browser"] },
        "tools": [{
            "name": "drive",
            "description": "Open a page in the Browser tab",
            "input": "schemas/any.json",
            "scope": "all"
        }]
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(
        root.join("main.js"),
        r#"import { View } from "gpui-kit";
import { next, reply } from "kagan/jobs";
import * as browser from "kagan/browser";
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          if (job.name !== "drive") {
            reply(job.id, { text: "skip" });
            continue;
          }
          const view = await browser.open(job.threadId, job.input.url);
          reply(job.id, { text: JSON.stringify(view) });
        } catch (error) {
          reply(job.id, { text: String(error), isError: true });
        }
      }
    });
  }
  render() { return "drive"; }
}
"#,
    )
    .unwrap();
    std::fs::create_dir(root.join("schemas")).unwrap();
    std::fs::write(root.join("schemas/any.json"), r#"{"type":"object"}"#).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(DRIVE.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({
                "powers": { "areas": ["browser"] },
                "downloads": {},
                "folders": {}
            }),
            error: None,
        })
        .unwrap();
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn browser_labels(cx: &mut TestAppContext, window: gpui_kit::AnyWindowHandle) -> Vec<String> {
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::render_frame(window, cx);
        crate::ui_dispatch::snapshot(window, 0)
            .elements
            .into_iter()
            .filter(|element| element.visible && element.label.as_deref() == Some("Browser"))
            .map(|element| element.id)
            .collect()
    })
    .unwrap()
}

fn with_sh(cx: &mut TestAppContext) {
    let path = std::env::var_os("PATH").unwrap_or_default();
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(
                std::path::Path::new(&path),
                std::path::Path::new("/bin/sh"),
            );
        });
    });
}

fn answer(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    let (sender, receive) = async_channel::bounded(1);
    cx.foreground_executor()
        .spawn(async move {
            let _ = sender.send(task.await).await;
        })
        .detach();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(12) {
        cx.run_until_parked();
        if let Ok(result) = receive.try_recv() {
            return result;
        }
        std::thread::sleep(Duration::from_millis(20));
        if start.elapsed() > Duration::from_secs(8) {
            cx.executor().advance_clock(Duration::from_millis(200));
        }
    }
    panic!("the browser call did not answer");
}

#[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
fn serve_page() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let body =
            b"<html><head><title>Driven page</title></head><body><button>Go</button></body></html>";
        let header = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        for _ in 0..8 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(body);
        }
    });
    port
}

#[gpui_kit::test]
fn a_plugin_opens_the_built_in_browser_and_adds_no_panel(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_drive);
    with_sh(cx);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    click(cx, window, "right-tab-add");
    assert!(present(cx, window, "right-add-browser"));
    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let labels = browser_labels(cx, window);
        assert_eq!(labels.len(), 1, "{labels:?}");
        assert!(
            labels[0].contains("right-add-browser"),
            "the plus menu should list one Browser: {labels:?}"
        );
    }
    cx.dispatch_action(window, gpui_kit::base::actions::Cancel);
    cx.run_until_parked();

    #[cfg(any(target_os = "linux", target_os = "macos", target_os = "windows"))]
    {
        let port = serve_page();
        let url = format!("http://127.0.0.1:{port}/");
        let OpResult::ToolReply { text, is_error } = answer(
            cx,
            Op::Thread(ThreadOp::CallTool {
                id: id.clone(),
                name: "drive".into(),
                input: json!({ "url": url }),
            }),
        )
        .unwrap() else {
            panic!("tool reply");
        };
        if is_error {
            // The test window has no native page. The call still reached that opener.
            assert!(
                text.contains("The page could not open") || text.contains("X11 window"),
                "{text}"
            );
            assert!(!shows(cx, window, "browser-driven"));
        } else {
            assert!(text.contains("Driven page"), "{text}");
            assert!(shows(cx, window, "browser-driven"));
            assert_eq!(selected_of(cx, window, "right-tab-browser"), Some(true));
            assert_eq!(disabled_of(cx, window, "browser-back"), Some(false));
            click(cx, window, "right-tab-add");
            assert!(!present(cx, window, "right-add-browser"));
            assert!(present(cx, window, "right-add-terminal"));
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let OpResult::ToolReply { text, is_error } = answer(
            cx,
            Op::Thread(ThreadOp::CallTool {
                id,
                name: "drive".into(),
                input: json!({ "url": "https://example.com" }),
            }),
        )
        .unwrap() else {
            panic!("tool reply");
        };
        assert!(is_error, "{text}");
        assert!(
            text.contains("The browser works on macOS, Windows and Linux only for now."),
            "{text}"
        );
        assert!(!shows(cx, window, "browser-driven"));
    }
}
