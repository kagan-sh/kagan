use super::{Body, Item, Plan, Step};
use kg_domain::{ThreadId, Timestamp};

// The two kinds this version cannot write are still typed for the wire, so a plugin reads one
// shape whatever slice fills them.
#[test]
fn a_thought_and_a_plan_read_as_the_d_ts_declares() {
    let item = |body| Item {
        seq: 1,
        thread_id: ThreadId("thr_1".into()),
        at: Timestamp(2),
        body,
    };
    let thought = serde_json::to_value(item(Body::Thought {
        text: "hmm".into(),
        done: false,
    }))
    .unwrap();
    assert_eq!(thought["kind"], "thought");
    assert_eq!(thought["text"], "hmm");
    assert_eq!(thought["done"], false);

    let plan = serde_json::to_value(item(Body::Plan {
        plan: Plan {
            steps: vec![Step {
                step: "read the log".into(),
                status: "running",
            }],
        },
    }))
    .unwrap();
    assert_eq!(plan["kind"], "plan");
    assert_eq!(plan["plan"]["steps"][0]["step"], "read the log");
    assert_eq!(plan["plan"]["steps"][0]["status"], "running");
}
