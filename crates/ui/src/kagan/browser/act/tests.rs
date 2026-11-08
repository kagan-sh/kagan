use super::{FILE_REFUSAL, click, fill, press, script, scroll, select, type_text};
use crate::kagan::browser::snapshot::parse_info;
use kg_domain::{BrowserOp, OpError, ThreadId};

#[test]
fn a_text_argument_stays_inside_a_json_string() {
    let text = "say \"hi\"\n</script>`date`";
    let encoded = serde_json::to_string(text).expect("a JSON string");
    for script in [fill(2, text), type_text(text), select(2, text)] {
        assert!(script.contains(&encoded), "{script}");
        assert!(!script.contains("say \"hi\""), "a quote stays encoded");
        assert!(
            !script.contains("hi\"\n</script>"),
            "a newline stays encoded"
        );
    }
}

#[test]
fn a_dead_ref_asks_for_a_fresh_snapshot() {
    for script in [click(7), fill(7, "x"), select(7, "x")] {
        assert!(
            script.contains("take a fresh snapshot"),
            "a dead ref says how to recover: {script}"
        );
    }
    assert_eq!(
        parse_info(
            r#"{"ok":false,"code":"not-found","message":"ref 7 is not on the page; take a fresh snapshot"}"#
        ),
        Err(OpError::NotFound(
            "ref 7 is not on the page; take a fresh snapshot".into()
        ))
    );
}

#[test]
fn a_file_input_is_refused_before_any_event() {
    for script in [click(1), fill(1, "x")] {
        let refusal = script.find(FILE_REFUSAL).expect("the refusal");
        let event = script.find("pointerdown").or_else(|| script.find("put("));
        assert!(refusal < event.expect("an event or a write"), "{script}");
    }
}

#[test]
fn a_wrong_option_lists_up_to_ten_labels() {
    let script = select(2, "nope");
    assert!(script.contains("slice(0, 10)"), "{script}");
    assert!(script.contains("choose one of: "), "{script}");
    assert_eq!(
        parse_info(
            r#"{"ok":false,"code":"invalid","message":"no option \"nope\"; choose one of: Red, Blue"}"#
        ),
        Err(OpError::Invalid(
            "no option \"nope\"; choose one of: Red, Blue".into()
        ))
    );
}

#[test]
fn press_carries_the_key_code_and_key_code() {
    let enter = press("enter").expect("enter");
    assert!(enter.contains(r#"key: "Enter""#));
    assert!(enter.contains(r#"code: "Enter""#));
    assert!(enter.contains("keyCode: 13"));
    let up = press("arrowup").expect("an arrow");
    assert!(up.contains(r#"key: "ArrowUp""#));
    assert!(up.contains("keyCode: 38"));
    let letter = press("a").expect("a letter");
    assert!(letter.contains(r#"key: "a""#));
    assert!(letter.contains(r#"code: "KeyA""#));
    assert!(letter.contains("keyCode: 65"));
    let space = press("space").expect("space");
    assert!(space.contains(r#"key: " ""#));
    assert!(space.contains(r#"code: "Space""#));
    assert!(space.contains("keyCode: 32"));
    assert!(press("f13").is_err());
    assert!(press("").is_err());
    assert!(press(" ").is_ok());
    assert!(press("\t").is_err());
}

#[test]
fn scroll_embeds_a_finite_pixel_count() {
    for (dy, want) in [
        (-240.0, "top: -240,"),
        (0.0, "top: 0,"),
        (10_000.0, "top: 10000,"),
    ] {
        let script = scroll(dy);
        assert!(script.contains(want), "{script}");
        assert!(!script.contains("__DY__"), "{script}");
    }
    assert!(scroll(f32::INFINITY).contains("top: 0,"));
}

#[test]
fn script_maps_the_acting_verbs_and_refuses_the_rest() {
    let thread = || ThreadId("thr_1".into());
    let built = |op: BrowserOp| script(&op).expect("a script");
    assert!(
        built(BrowserOp::Click {
            ref_id: 1,
            thread_id: thread()
        })
        .contains("pointerdown")
    );
    assert!(
        built(BrowserOp::Fill {
            ref_id: 1,
            text: Some("x".into()),
            file: None,
            thread_id: thread(),
        })
        .contains("put(element,")
    );
    assert!(
        built(BrowserOp::Type {
            text: "x".into(),
            thread_id: thread()
        })
        .contains("insertText")
    );
    assert!(
        built(BrowserOp::Press {
            key: "enter".into(),
            thread_id: thread()
        })
        .contains(r#"key: "Enter""#)
    );
    assert!(
        built(BrowserOp::Select {
            ref_id: 1,
            option: "x".into(),
            thread_id: thread(),
        })
        .contains("options.find")
    );
    assert!(
        built(BrowserOp::Scroll {
            dy: 5.0,
            thread_id: thread()
        })
        .contains("scrollBy")
    );
    assert_eq!(
        script(&BrowserOp::Open {
            url: "x".into(),
            thread_id: thread()
        }),
        Err("this browser verb does not act on the page".into())
    );
    assert!(
        script(&BrowserOp::Press {
            key: "f13".into(),
            thread_id: thread()
        })
        .is_err()
    );
}

#[test]
fn a_text_action_never_relays_the_page_exception() {
    for script in [fill(1, "hunter2"), type_text("hunter2")] {
        assert!(script.contains("the page refused the text"), "{script}");
        assert!(!script.contains("error && error.message"), "{script}");
    }
}
