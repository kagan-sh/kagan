use super::{EnvOp, variable_name, variable_value};
use crate::{Op, ProjectId, ThreadId, check};

#[test]
fn names_follow_the_shell_and_skip_kagans_own() {
    for good in ["API_KEY", "_x", "a1", "lower_case"] {
        assert!(variable_name(good).is_ok(), "{good} should be accepted");
    }
    for bad in [
        "",
        "1X",
        "A-B",
        "A B",
        "A=B",
        "KG_THREAD_ID",
        "kg_x",
        "PATH",
        "Path",
    ] {
        assert!(variable_name(bad).is_err(), "{bad} should be refused");
    }
}

#[test]
fn values_are_one_nonempty_line_under_the_limit() {
    assert!(variable_value("sk-123").is_ok());
    assert!(variable_value("").is_err());
    assert!(variable_value("a\nb").is_err());
    assert!(variable_value("a\0b").is_err());
    assert!(variable_value(&"x".repeat(1025)).is_err());
    assert!(variable_value(&"x".repeat(1024)).is_ok());
}

#[test]
fn a_set_without_a_value_is_checked_on_the_name_only() {
    let set = |name: &str, value: Option<&str>| {
        Op::Env(EnvOp::Set {
            name: name.into(),
            project_id: None,
            value: value.map(Into::into),
            thread_id: None,
        })
    };
    assert!(check(&set("TOKEN", None)).is_ok());
    assert!(check(&set("KG_TOKEN", None)).is_err());
    assert!(check(&set("TOKEN", Some(""))).is_err());
}

#[test]
fn a_removal_may_name_a_kagan_name_it_never_stored() {
    let remove = Op::Env(EnvOp::Remove {
        name: "KG_OLD".into(),
        project_id: Some(ProjectId("prj_1".into())),
        thread_id: None,
    });
    assert!(check(&remove).is_ok());
}

#[test]
fn a_thread_may_list_but_not_change_variables() {
    let thread = Some(ThreadId("thr_program".into()));
    let set = Op::Env(EnvOp::Set {
        name: "TOKEN".into(),
        project_id: None,
        value: Some("x".into()),
        thread_id: thread.clone(),
    });
    let remove = Op::Env(EnvOp::Remove {
        name: "TOKEN".into(),
        project_id: None,
        thread_id: thread.clone(),
    });
    let list = Op::Env(EnvOp::List { project_id: None });
    assert!(check(&set).is_err());
    assert!(check(&remove).is_err());
    assert!(check(&list).is_ok());
}

#[test]
fn debug_never_shows_the_value() {
    let op = EnvOp::Set {
        name: "TOKEN".into(),
        project_id: None,
        value: Some("sk-secret-123".into()),
        thread_id: None,
    };
    let text = format!("{op:?}");
    assert!(text.contains("TOKEN"));
    assert!(!text.contains("sk-secret-123"));
}

#[test]
fn a_set_value_is_not_on_the_wire_when_unset() {
    let op = EnvOp::Set {
        name: "TOKEN".into(),
        project_id: None,
        value: None,
        thread_id: None,
    };
    let wire = serde_json::to_value(Op::Env(op)).unwrap();
    assert!(wire["env"]["set"].get("value").is_none());
    assert!(wire["env"]["set"].get("threadId").is_none());
}
