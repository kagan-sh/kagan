use gpui_shell::HostArguments;
use kg_domain::BrowserOp::{
    Back, Click, Close, Fill, Forward, Goto, Open, Press, Reload, Scroll, Select, Snapshot, Status,
    Type,
};
use kg_domain::{BrowserOp, ThreadId};
use serde_json::{Value, json};

use crate::area::to_host;
use crate::browser::CALLS;

fn build(name: &str, args: Vec<Value>) -> Result<BrowserOp, String> {
    let (_, build) = CALLS.iter().find(|(call, _)| *call == name).unwrap();
    build(&HostArguments::new(args.into_iter().map(to_host))).map_err(|error| error.to_string())
}

fn ok(name: &str, args: Value) -> BrowserOp {
    let Value::Array(args) = args else {
        unreachable!("arguments are a list")
    };
    build(name, args).unwrap()
}

#[test]
fn a_call_with_only_a_thread_builds_its_own_operation() {
    let id = || ThreadId("thr_1".into());
    let cases = [
        ("status", Status { thread_id: id() }),
        ("snapshot", Snapshot { thread_id: id() }),
        ("back", Back { thread_id: id() }),
        ("forward", Forward { thread_id: id() }),
        ("reload", Reload { thread_id: id() }),
        ("close", Close { thread_id: id() }),
    ];
    for (name, expected) in cases {
        assert_eq!(ok(name, json!(["thr_1"])), expected, "{name}");
    }
}

#[test]
fn a_call_with_more_arguments_builds_its_own_operation() {
    let id = || ThreadId("thr_1".into());
    let url = |text: &str| text.to_owned();
    assert_eq!(
        ok("open", json!(["thr_1", "https://a.test"])),
        Open {
            thread_id: id(),
            url: url("https://a.test")
        }
    );
    assert_eq!(
        ok("goto", json!(["thr_1", "https://b.test"])),
        Goto {
            thread_id: id(),
            url: url("https://b.test")
        }
    );
    assert_eq!(
        ok("click", json!(["thr_1", 7])),
        Click {
            thread_id: id(),
            ref_id: 7
        }
    );
    assert_eq!(
        ok("fill", json!(["thr_1", 7, "hello"])),
        Fill {
            thread_id: id(),
            ref_id: 7,
            text: Some(url("hello")),
            file: None
        }
    );
    assert_eq!(
        ok("type", json!(["thr_1", "abc"])),
        Type {
            thread_id: id(),
            text: url("abc")
        }
    );
    assert_eq!(
        ok("press", json!(["thr_1", "Enter"])),
        Press {
            thread_id: id(),
            key: url("Enter")
        }
    );
    assert_eq!(
        ok("select", json!(["thr_1", 7, "two"])),
        Select {
            thread_id: id(),
            ref_id: 7,
            option: url("two")
        }
    );
    assert_eq!(
        ok("scroll", json!(["thr_1", 12.5])),
        Scroll {
            thread_id: id(),
            dy: 12.5
        }
    );
}

#[test]
fn scroll_takes_a_finite_number_and_click_a_whole_one() {
    assert_eq!(
        build("scroll", vec![json!("thr_1"), json!(-3.0)]).unwrap(),
        Scroll {
            thread_id: ThreadId("thr_1".into()),
            dy: -3.0
        }
    );
    for bad in [json!("far"), json!(null), json!(1e300)] {
        assert_eq!(
            build("scroll", vec![json!("thr_1"), bad]).unwrap_err(),
            "scroll must be a number"
        );
    }
    for bad in [json!(1.5), json!(-1), json!("1"), json!(1e10)] {
        assert_eq!(
            build("click", vec![json!("thr_1"), bad]).unwrap_err(),
            "ref must be a whole number"
        );
    }
}
