use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, Plugin, PluginId, SurfaceItem, SurfaceOp};
use serde_json::json;

const ALPHA: &str = "dev.test.alpha";
const BETA: &str = "dev.test.beta";
const BROKEN: &str = "dev.test.broken";

pub(in crate::tests) fn install_many(data: &Path) {
    let mut panels = Vec::new();
    for index in 0..12 {
        panels.push(json!({
            "id": format!("t{index}"),
            "title": format!("Tool {index}"),
            "view": "ToolPanel",
            "placement": "left"
        }));
    }
    panels.push(json!({
        "id": "page",
        "title": "Board",
        "view": "PagePanel",
        "placement": "center"
    }));
    write_plugin(
        data,
        "many",
        "dev.test.many",
        &manifest("dev.test.many", "Many", json!(panels), false),
        &source(false),
    );
}

pub(in crate::tests) fn install_pair(data: &Path) {
    write_plugin(
        data,
        "alpha",
        ALPHA,
        &manifest(
            ALPHA,
            "Alpha",
            json!([
                {"id": "tool", "title": "Alpha tool", "view": "ToolPanel", "placement": "left"},
                {"id": "page", "title": "Alpha page", "view": "PagePanel", "placement": "center"},
                {"id": "side", "title": "Alpha side", "view": "SidePanel", "placement": "right"}
            ]),
            true,
        ),
        &source(true),
    );
    write_plugin(
        data,
        "beta",
        BETA,
        &manifest(
            BETA,
            "Beta",
            json!([
                {"id": "tool", "title": "Beta tool", "view": "ToolPanel", "placement": "left"},
                {"id": "page", "title": "Beta page", "view": "PagePanel", "placement": "center"}
            ]),
            false,
        ),
        &source(false),
    );
}

pub(in crate::tests) fn install_broken(data: &Path) {
    write_plugin(
        data,
        "broken",
        BROKEN,
        &manifest(
            BROKEN,
            "Broken",
            json!([
                {"id": "page", "title": "Broken page", "view": "PagePanel", "placement": "center"}
            ]),
            false,
        ),
        BROKEN_SOURCE,
    );
}

pub(in crate::tests) fn manifest(
    id: &str,
    name: &str,
    panels: serde_json::Value,
    command: bool,
) -> serde_json::Value {
    let mut value = json!({
        "id": id,
        "name": name,
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A surface fixture.",
        "powers": {},
        "ui": {}
    });
    value["ui"]["panels"] = panels;
    if command {
        value["commands"] = json!([{ "id": "open", "title": "Open", "when": "always" }]);
    }
    value
}

pub(in crate::tests) fn source(command: bool) -> String {
    let handle = if command {
        r#"async function handle(job) {
  if (job.kind === "command" && job.command === "open") {
    await openPanel("tool", { mark: "from-host" });
    return null;
  }
  throw new Error("unknown");
}"#
    } else {
        r#"async function handle(job) {
  throw new Error("unknown " + job.kind);
}"#
    };
    format!(
        r#"// @ts-check
import {{ View, div }} from "gpui-kit";
import {{ Input, InputState, Popover }} from "gpui-base";
import {{ KgButton }} from "gpui-component";
import {{ context, onContextChanged, openPanel }} from "kagan/host";
import {{ fail, next, reply }} from "kagan/jobs";
export default class Plugin extends View {{
  init(_props, cx) {{
    cx.spawn(async () => {{
      for (;;) {{
        const job = await next();
        handle(job).then((value) => reply(job.id, value), (error) => fail(job.id, String(error)));
      }}
    }});
  }}
  render() {{ return ""; }}
}}
{tool}
{page}
{side}
{handle}
"#,
        tool = panel_class("ToolPanel", "alpha-tool", "alpha-ready", false),
        page = panel_class("PagePanel", "alpha-page", "page-ready", true),
        side = panel_class("SidePanel", "alpha-side", "side-ready", false),
        handle = handle,
    )
    .replace(
        "alpha-tool",
        if command { "alpha-tool" } else { "beta-tool" },
    )
    .replace(
        "alpha-ready",
        if command { "alpha-ready" } else { "beta-ready" },
    )
    .replace(
        "alpha-page",
        if command { "alpha-page" } else { "beta-page" },
    )
    .replace(
        "page-ready",
        if command {
            "page-ready"
        } else {
            "beta-page-ready"
        },
    )
    .replace(
        "alpha-side",
        if command { "alpha-side" } else { "beta-side" },
    )
}

pub(in crate::tests) fn panel_class(name: &str, button: &str, word: &str, page: bool) -> String {
    let field = if page {
        r#"this.pop = false;
    this.field = InputState.new({ value: "" });
    this.typed = "";
    this.field.on("change", (_event, cx) => {
      this.typed = this.field.value();
      cx.notify();
    });"#
    } else {
        ""
    };
    let extra = if page {
        format!(
            r#".child(Input.new(this.field)).child(new KgButton("{button}-typed").label(this.typed || "empty")).child(Popover.new("page-pop").open(this.pop).on_open_change((open, cx) => {{ this.pop = open; cx.notify(); }}).trigger(new KgButton("{button}-pop-open").label("Open pop")).content(new KgButton("{button}-pop").label("pop")))"#
        )
    } else {
        String::new()
    };
    format!(
        r#"export class {name} extends View {{
  init(_props, cx) {{
    this.word = "{word}";
    this.count = 0;
    {field}
    cx.spawn(async () => {{
      const read = (value) => {{
        if (value && value.props && value.props.mark) this.word = String(value.props.mark);
      }};
      read(context());
      cx.notify();
      for (;;) {{
        read(await onContextChanged());
        cx.notify();
      }}
    }});
  }}
  render() {{
    return div()
      {extra}
      .child(new KgButton("{button}").label(this.word))
      .child(new KgButton("{button}-count").label(String(this.count)).on_click((_event, cx) => {{
        this.count++;
        cx.notify();
      }}));
  }}
}}"#
    )
}

pub(in crate::tests) const BROKEN_SOURCE: &str = r#"// @ts-check
import { View } from "gpui-kit";
export default class Plugin extends View {}
export class PagePanel extends View {
  render() { throw new Error("boom in render"); }
}
"#;

pub(in crate::tests) fn write_plugin(
    data: &Path,
    folder: &str,
    id: &str,
    manifest: &serde_json::Value,
    entry: &str,
) {
    let root = data.join(folder);
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), entry).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(id.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({"powers": {}, "downloads": {}, "folders": {}}),
            skills: Vec::new(),
            error: None,
        })
        .unwrap();
}

pub(in crate::tests) fn wide(cx: &mut TestAppContext, window: AnyWindowHandle) {
    cx.simulate_window_resize(window, size(px(1400.), px(900.)));
    cx.run_until_parked();
}

pub(in crate::tests) fn run(cx: &mut TestAppContext, op: Op) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.foreground_executor().block_test(task)
}

pub(in crate::tests) fn surfaces(cx: &mut TestAppContext) -> Vec<SurfaceItem> {
    match run(cx, Op::Surface(SurfaceOp::List)).unwrap() {
        OpResult::Surfaces(items) => items,
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn open_surface(
    cx: &mut TestAppContext,
    plugin: &str,
    id: &str,
) -> Result<OpResult, OpError> {
    run(
        cx,
        Op::Surface(SurfaceOp::Open {
            plugin: PluginId(plugin.into()),
            id: id.into(),
        }),
    )
}

pub(in crate::tests) fn close_surface(
    cx: &mut TestAppContext,
    plugin: &str,
    id: &str,
) -> Result<OpResult, OpError> {
    run(
        cx,
        Op::Surface(SurfaceOp::Close {
            plugin: PluginId(plugin.into()),
            id: id.into(),
        }),
    )
}

pub(in crate::tests) fn item<'a>(
    items: &'a [SurfaceItem],
    plugin: &str,
    id: &str,
) -> &'a SurfaceItem {
    items
        .iter()
        .find(|item| item.plugin.0 == plugin && item.id == id)
        .unwrap_or_else(|| panic!("{plugin} {id} missing from {items:?}"))
}

pub(in crate::tests) fn width_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(0., |element| f32::from(element.bounds.size.width))
    })
    .unwrap()
}

pub(in crate::tests) fn height_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .map_or(0., |element| f32::from(element.bounds.size.height))
    })
    .unwrap()
}

pub(in crate::tests) fn body(plugin: &str, id: &str) -> String {
    format!("panel-{plugin}-{id}-body")
}

pub(in crate::tests) fn box_of(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> (f32, f32, f32, f32) {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into())).map_or(
            (0., 0., 0., 0.),
            |element| {
                let bounds = element.bounds;
                (
                    f32::from(bounds.origin.x),
                    f32::from(bounds.origin.y),
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                )
            },
        )
    })
    .unwrap()
}
