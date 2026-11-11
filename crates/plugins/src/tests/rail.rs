use std::path::Path;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, PluginId, PluginOp, PreferenceOp, RailAction, prefs};
use serde_json::{Value, json};

use super::{answer_standalone, installed_at, open, plugin_op, write_plugin};

const RAIL: &str = "dev.test.rail";

fn plugin(root: &Path) {
    write_plugin(
        root,
        &json!({
            "id": RAIL,
            "name": "Rail",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "instructions": "Has a rail icon.",
            "powers": {}
        }),
    );
}

fn hidden(cx: &mut TestAppContext) -> Value {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Get {
                key: prefs::RAIL_HIDDEN.into(),
            }),
            cx,
        )
    });
    match cx.foreground_executor().block_test(task).unwrap() {
        OpResult::Setting(value) => value,
        other => panic!("{other:?}"),
    }
}

fn rail(cx: &mut TestAppContext, id: &str, action: RailAction) -> Result<OpResult, OpError> {
    let task = plugin_op(
        cx,
        PluginOp::Rail {
            id: PluginId(id.into()),
            action,
        },
    );
    cx.foreground_executor().block_test(task)
}

#[gpui_kit::test]
fn hiding_is_a_saved_list_of_plugin_ids_and_showing_removes_it(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    plugin(dir.path());
    open(cx, &[installed_at(dir.path())]);
    assert_eq!(hidden(cx), Value::Null);

    assert_eq!(
        rail(cx, RAIL, RailAction::Hide),
        Ok(OpResult::Setting(json!([RAIL])))
    );
    assert_eq!(
        rail(cx, RAIL, RailAction::Hide),
        Ok(OpResult::Setting(json!([RAIL])))
    );
    assert_eq!(hidden(cx), json!([RAIL]), "hiding twice keeps one entry");

    assert_eq!(
        rail(cx, RAIL, RailAction::Show),
        Ok(OpResult::Setting(json!([])))
    );
    assert_eq!(hidden(cx), json!([]));
}

#[gpui_kit::test]
fn a_plugin_that_is_not_installed_cannot_be_hidden(cx: &mut TestAppContext) {
    open(cx, &[]);
    let refused = rail(cx, "dev.test.missing", RailAction::Hide);
    assert!(matches!(refused, Err(OpError::NotFound(_))), "{refused:?}");
    assert_eq!(hidden(cx), Value::Null);
}

#[gpui_kit::test]
fn removing_a_plugin_drops_its_rail_entry(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    plugin(dir.path());
    open(cx, &[installed_at(dir.path())]);
    rail(cx, RAIL, RailAction::Hide).unwrap();
    assert_eq!(hidden(cx), json!([RAIL]));

    let id = PluginId(RAIL.into());
    let removed = plugin_op(
        cx,
        PluginOp::Remove {
            id,
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removed).unwrap();
    assert_eq!(hidden(cx), json!([]), "remove left the rail entry");
}
