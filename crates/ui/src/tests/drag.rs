use std::path::Path;

use gpui_kit::{AnyWindowHandle, TestAppContext};
use kg_core::Workspace;
use kg_domain::{DropSide, Op, OpError, OpResult, Plugin, PluginId, SurfaceOp, UiOp};

use super::kagan::{label, open_with_plugins_then, shows, wait_until};

const BOARD: &str = "dev.test.board";

/// Two columns of cards, drawn as a center page. `board-log` shows what the script heard, which
/// card looks dragged, and where on which card the pointer is.
const SOURCE: &str = r#"// @ts-check
import { View, div } from "gpui-kit";
import { h_flex, v_flex } from "gpui-base";
import { KgButton } from "gpui-component";
export default class Plugin extends View {}
export class Board extends View {
  init() {
    this.log = [];
    this.dragged = "-";
    this.over = "-";
  }
  note(line, cx) {
    this.log.push(line);
    cx.notify();
  }
  card(id) {
    return div()
      .id(id)
      .h(40)
      .when(this.dragged === id, (el) => el.opacity(0.5))
      .on_drag(`ticket:${id}`, (event, cx) => {
        this.dragged = event.phase === "started" ? id : "-";
        this.note(`drag:${event.phase}`, cx);
      })
      .on_drag_move((event, cx) => {
        this.over = event.side ? `${id}:${event.side}` : "-";
        cx.notify();
      })
      .on_drop((event, cx) => this.note(`drop:${id}:${event.payload}:${event.side}`, cx))
      .child(new KgButton(`${id}-open`).label(id).on_click((_event, cx) => this.note(`open:${id}`, cx)));
  }
  render() {
    return v_flex()
      .size_full()
      .child(
        h_flex()
          .h(200)
          .child(v_flex().id("todo").w(150).h(200).child(this.card("card-a")).child(this.card("card-b")))
          .child(
            v_flex()
              .id("done")
              .w(150)
              .h(200)
              .on_drop((event, cx) => this.note(`drop:done:${event.payload}`, cx)),
          ),
      )
      .child(new KgButton("board-log").label(`${this.log.join(" ")}|${this.dragged}|${this.over}`));
  }
}
"#;

fn install_board(data: &Path) {
    let root = data.join("board");
    std::fs::create_dir(&root).unwrap();
    let manifest = serde_json::json!({
        "id": BOARD,
        "name": "Board",
        "version": "1.0.0",
        "kagan": ">=0.1.0",
        "entry": "main.js",
        "instructions": "A test board.",
        "powers": {},
        "ui": {"panels": [{"id": "board", "title": "Board", "view": "Board", "placement": "center"}]}
    });
    std::fs::write(root.join("kagan.json"), manifest.to_string()).unwrap();
    std::fs::write(root.join("main.js"), SOURCE).unwrap();
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(BOARD.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: serde_json::json!({"powers": {}, "downloads": {}, "folders": {}}),
            error: None,
        })
        .unwrap();
}

fn open_board(cx: &mut TestAppContext) -> (AnyWindowHandle, tempfile::TempDir) {
    let (_kagan, window, data) = open_with_plugins_then(cx, install_board);
    cx.run_until_parked();
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Surface(SurfaceOp::Open {
                plugin: PluginId(BOARD.into()),
                id: "board".into(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| shows(cx, window, "card-a"));
    (window, data)
}

fn ui(cx: &mut TestAppContext, op: UiOp) -> Result<OpResult, OpError> {
    let task = cx.update(|cx| Workspace::dispatch(Op::Ui(op), cx));
    let result = cx.foreground_executor().block_test(task);
    cx.run_until_parked();
    result
}

fn drag(to: &str, at: DropSide, hold: bool) -> UiOp {
    UiOp::Drag {
        from: "card-a".into(),
        to: to.into(),
        at,
        within: None,
        hold,
        window: 0,
    }
}

#[gpui_kit::test]
fn kg_ui_drags_a_card_onto_the_top_of_another(cx: &mut TestAppContext) {
    let (window, _data) = open_board(cx);
    assert_eq!(
        ui(cx, drag("card-b", DropSide::Top, false)),
        Ok(OpResult::Done)
    );
    assert_eq!(
        label(cx, window, "board-log"),
        "drag:started drop:card-b:ticket:card-a:top drag:ended|-|-"
    );
}

/// A held drag shows the source as dragged and the target as entered, so a snapshot or a
/// screenshot can check both, and `kg ui drop` finishes it.
#[gpui_kit::test]
fn a_held_drag_shows_its_state_and_kg_ui_drop_finishes_it(cx: &mut TestAppContext) {
    let (window, _data) = open_board(cx);
    ui(cx, drag("card-b", DropSide::Bottom, true)).unwrap();
    assert_eq!(
        label(cx, window, "board-log"),
        "drag:started|card-a|card-b:bottom"
    );

    assert_eq!(ui(cx, UiOp::Drop { window: 0 }), Ok(OpResult::Done));
    assert_eq!(
        label(cx, window, "board-log"),
        "drag:started drop:card-b:ticket:card-a:bottom drag:ended|-|-"
    );
}

/// Escape cancels a held drag and stays the drag's: the page under it does not close.
#[gpui_kit::test]
fn escape_cancels_a_held_drag_and_leaves_the_page_open(cx: &mut TestAppContext) {
    let (window, _data) = open_board(cx);
    ui(cx, drag("card-b", DropSide::Middle, true)).unwrap();
    assert_eq!(
        ui(
            cx,
            UiOp::Key {
                keystroke: "escape".into(),
                window: 0
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(
        label(cx, window, "board-log"),
        "drag:started drag:cancelled|-|-"
    );
    assert!(shows(cx, window, "surface-page"));
    assert_eq!(
        ui(cx, UiOp::Drop { window: 0 }),
        Err(OpError::Invalid(
            "no drag is held; start one with kg ui drag --hold".into()
        ))
    );
    // With no drag, Escape is the page's again.
    ui(
        cx,
        UiOp::Key {
            keystroke: "escape".into(),
            window: 0,
        },
    )
    .unwrap();
    wait_until(cx, |cx| !shows(cx, window, "surface-page"));
}

/// A button inside a source still clicks when nothing is dragged.
#[gpui_kit::test]
fn a_button_inside_a_card_still_clicks(cx: &mut TestAppContext) {
    let (window, _data) = open_board(cx);
    assert_eq!(
        ui(
            cx,
            UiOp::Click {
                element_id: "card-a-open".into(),
                within: None,
                window: 0,
            }
        ),
        Ok(OpResult::Done)
    );
    assert_eq!(label(cx, window, "board-log"), "open:card-a|-|-");
}

#[gpui_kit::test]
fn a_release_outside_every_target_drops_nothing(cx: &mut TestAppContext) {
    let (window, _data) = open_board(cx);
    ui(cx, drag("board-log", DropSide::Middle, false)).unwrap();
    assert_eq!(
        label(cx, window, "board-log"),
        "drag:started drag:cancelled|-|-"
    );
}

#[gpui_kit::test]
fn kg_ui_snapshot_lists_a_plugins_sources_and_targets(cx: &mut TestAppContext) {
    let (_window, _data) = open_board(cx);
    let Ok(OpResult::Ui(snapshot)) = ui(cx, UiOp::Snapshot { window: 0 }) else {
        panic!("a snapshot");
    };
    for id in ["card-a", "card-b", "done"] {
        assert!(
            snapshot
                .elements
                .iter()
                .any(|element| element.id == format!("{:?}", gpui_kit::ElementId::from(id))),
            "{id} is listed"
        );
    }
}
