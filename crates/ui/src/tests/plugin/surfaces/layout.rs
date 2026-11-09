use crate::kagan::screen_read;
use std::path::Path;

use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, PluginId, PluginOp, PreferenceOp};
use serde_json::json;

use super::{
    body, close_surface, install_pair, item, manifest, open_surface, run, surfaces, wide, width_of,
    write_plugin,
};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, press_key, run_shell, selected_of, shows,
    wait_until,
};

const ALPHA: &str = "dev.test.alpha";

#[gpui_kit::test]
fn full_screen_gives_the_right_sidebar_the_window_and_gives_it_back(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    let right = width_of(cx, window, "right-sidebar");

    click(cx, window, "right-full");
    assert!(!shows(cx, window, "center"));
    assert!(width_of(cx, window, "right-sidebar") > right);
    assert_eq!(selected_of(cx, window, "right-full"), Some(true));

    click(cx, window, "right-full");
    assert!(shows(cx, window, "center"));
    assert!((width_of(cx, window, "right-sidebar") - right).abs() < 0.5);

    click(cx, window, "right-full");
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert_eq!(selected_of(cx, window, "right-full"), Some(false));
}

pub(in crate::tests) const BLANK: &str = "dev.test.blank";

pub(in crate::tests) const BLANK_SOURCE: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
export default class Plugin extends View {
  render() { return ""; }
}
export class ToolPanel extends View {
  render() { return div(); }
}
export class SidePanel extends View {
  render() { return div(); }
}
export class PagePanel extends View {
  render() { return div(); }
}
"#;

pub(in crate::tests) const FILL: &str = "dev.test.fill";

pub(in crate::tests) const FILL_SOURCE: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
import { KgButton } from "gpui-component";
import { context, onContextChanged } from "kagan/host";
export default class Plugin extends View {
  render() { return ""; }
}
export class SidePanel extends View {
  init(_props, cx) {
    this.word = "";
    const apply = (value) => {
      const mark = value && value.props && value.props.mark;
      this.word = mark ? String(mark) : "";
    };
    apply(context());
    cx.spawn(async () => {
      for (;;) {
        apply(await onContextChanged());
        cx.notify();
      }
    });
  }
  render() {
    if (!this.word) return div();
    return div().child(new KgButton("fill-side").label(this.word));
  }
}
"#;

pub(in crate::tests) fn install_fill(data: &Path) {
    write_plugin(
        data,
        "fill",
        FILL,
        &manifest(
            FILL,
            "Fill",
            json!([
                {"id": "side", "title": "Fill side", "view": "SidePanel", "placement": "left"}
            ]),
            false,
        ),
        FILL_SOURCE,
    );
}

pub(in crate::tests) fn install_blank(data: &Path) {
    write_plugin(
        data,
        "blank",
        BLANK,
        &manifest(
            BLANK,
            "Blank",
            json!([
                {"id": "tool", "title": "Blank tool", "view": "ToolPanel", "placement": "left"},
                {"id": "side", "title": "Blank side", "view": "SidePanel", "placement": "right"},
                {"id": "page", "title": "Blank page", "view": "PagePanel", "placement": "center"}
            ]),
            false,
        ),
        BLANK_SOURCE,
    );
}

pub(in crate::tests) fn saved(cx: &mut TestAppContext, key: &str) -> serde_json::Value {
    let get = cx.update(|cx| {
        Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: key.into() }), cx)
    });
    match cx.foreground_executor().block_test(get).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

pub(in crate::tests) fn tab_to(cx: &mut TestAppContext, window: AnyWindowHandle, id: &str) {
    for _ in 0..80 {
        if focused_of(cx, window, id) == Some(true) {
            return;
        }
        cx.update_window(window, |_, window, cx| window.focus_next(cx))
            .unwrap();
        cx.run_until_parked();
    }
    panic!("tab did not reach {id}");
}

#[gpui_kit::test]
fn a_failed_open_keeps_the_saved_layout(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_blank);
    wide(cx, window);
    click(cx, window, "left-sidebar-toggle");
    assert!(!shows(cx, window, "project-prj_kagan"));
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
    let right = saved(cx, "main.right-sidebar");

    let missed = open_surface(cx, BLANK, "tool").unwrap_err();
    assert!(missed.to_string().contains("not on screen"), "{missed}");
    cx.run_until_parked();
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
    assert_eq!(saved(cx, "main.right-sidebar"), right);
    assert!(!shows(cx, window, "project-prj_kagan"));
    assert!(!item(&surfaces(cx), BLANK, "tool").open);

    open_surface(cx, BLANK, "side").unwrap();
    wait_until(cx, |cx| item(&surfaces(cx), BLANK, "side").open);
    assert!(item(&surfaces(cx), BLANK, "side").visible);
    assert_eq!(saved(cx, "main.left-sidebar"), json!(false));
}

#[gpui_kit::test]
fn opening_the_same_surface_again_keeps_its_view(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-tool-count"));
    click(cx, window, "alpha-tool-count");
    assert_eq!(label(cx, window, "alpha-tool-count"), "1");

    open_surface(cx, ALPHA, "tool").unwrap();
    assert_eq!(label(cx, window, "alpha-tool-count"), "1");
    assert_eq!(label(cx, window, "alpha-tool"), "alpha-ready");

    run(
        cx,
        Op::Plugin(PluginOp::Command {
            id: PluginId(ALPHA.into()),
            command: "open".into(),
            thread_id: None,
            project_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| label(cx, window, "alpha-tool") == "from-host");
    assert_eq!(label(cx, window, "alpha-tool-count"), "1");
}

#[gpui_kit::test]
fn enter_and_space_open_a_plugin_row(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let tool = format!("surface-item-{ALPHA}-tool");
    tab_to(cx, window, &tool);
    press_key(cx, window, "enter");
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    close_surface(cx, ALPHA, "tool").unwrap();
    tab_to(cx, window, &tool);
    press_key(cx, window, "space");
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    close_surface(cx, ALPHA, "tool").unwrap();
}

#[gpui_kit::test]
fn a_plugin_input_and_button_keep_the_page_open(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));

    cx.update_window(window, |_, window, cx| window.focus_next(cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_input(window, "hi");
    cx.run_until_parked();
    wait_until(cx, |cx| label(cx, window, "alpha-page-typed") == "hi");
    press_key(cx, window, "space");
    press_key(cx, window, "enter");
    assert!(shows(cx, window, "surface-page"));
    assert!(label(cx, window, "alpha-page-typed").contains("hi"));

    click(cx, window, "alpha-page-pop-open");
    wait_until(cx, |cx| shows(cx, window, "alpha-page-pop"));
    press_key(cx, window, "escape");
    assert!(shows(cx, window, "surface-page"));
    assert!(!shows(cx, window, "alpha-page-pop"));

    tab_to(cx, window, "alpha-page-count");
    press_key(cx, window, "enter");
    press_key(cx, window, "space");
    assert_eq!(label(cx, window, "alpha-page-count"), "2");
    assert!(shows(cx, window, "surface-page"));
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
}

pub(in crate::tests) fn install_pair_and_blank(data: &Path) {
    install_pair(data);
    install_blank(data);
}

pub(in crate::tests) fn terminal_focused(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    kagan: &gpui_kit::Entity<crate::kagan::Kagan>,
) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_view(kagan.read(cx)).is_some_and(|view| view.read(cx).focused(window))
    })
    .unwrap()
}

pub(in crate::tests) fn host_opens(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    opens: Vec<(&str, &str, serde_json::Value)>,
) -> Vec<async_channel::Receiver<Result<(), String>>> {
    cx.update_window(window, |_, _, cx| {
        let ui = kg_plugins::HostUi::global(cx).expect("host");
        let mut replies = Vec::new();
        for (plugin, panel, props) in opens {
            let (reply, incoming) = async_channel::bounded(1);
            ui.update(cx, |_, cx| {
                cx.emit(kg_plugins::HostUiEvent::OpenPanel {
                    plugin: PluginId(plugin.into()),
                    panel: panel.into(),
                    props,
                    reply,
                });
            });
            replies.push(incoming);
        }
        replies
    })
    .unwrap()
}

pub(in crate::tests) fn replied(
    receiver: &async_channel::Receiver<Result<(), String>>,
) -> Result<(), String> {
    receiver
        .try_recv()
        .unwrap_or_else(|error| panic!("no reply: {error}"))
}

pub(in crate::tests) fn show_thread(cx: &mut TestAppContext, window: AnyWindowHandle) {
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
}

pub(in crate::tests) fn focus_page_input(cx: &mut TestAppContext, window: AnyWindowHandle) {
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));
    cx.update_window(window, |_, window, cx| window.focus_next(cx))
        .unwrap();
    cx.run_until_parked();
    cx.simulate_input(window, "hi");
    cx.run_until_parked();
    wait_until(cx, |cx| label(cx, window, "alpha-page-typed") == "hi");
}

#[gpui_kit::test]
fn overlapping_host_opens_keep_the_first_request(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair_and_blank);
    wide(cx, window);

    let replies = host_opens(
        cx,
        window,
        vec![
            (ALPHA, "tool", json!({"mark": "one"})),
            (ALPHA, "page", json!({"mark": "two"})),
        ],
    );
    cx.run_until_parked();
    let first = replied(&replies[0]);
    assert!(first.is_ok(), "{first:?}");
    let refused = replied(&replies[1]).unwrap_err();
    assert!(refused.contains("already waiting"), "{refused}");
    wait_until(cx, |cx| label(cx, window, "alpha-tool") == "one");
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);
    assert!(!item(&surfaces(cx), ALPHA, "page").open);

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(BLANK.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
    click(cx, window, "right-sidebar-toggle");
    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(BLANK.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();

    let replies = host_opens(
        cx,
        window,
        vec![
            (BLANK, "tool", json!({"mark": "gone"})),
            (ALPHA, "page", json!({"mark": "nope"})),
        ],
    );
    cx.run_until_parked();
    let missed = replied(&replies[0]).unwrap_err();
    assert!(missed.contains("not on screen"), "{missed}");
    let refused = replied(&replies[1]).unwrap_err();
    assert!(refused.contains("already waiting"), "{refused}");
    assert_eq!(label(cx, window, "alpha-tool"), "one");
    assert!(!item(&surfaces(cx), ALPHA, "page").open);
    assert!(!shows(cx, window, &body(BLANK, "tool")));
}
