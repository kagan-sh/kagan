use super::{parse_info, parse_snapshot};
use kg_domain::OpError;

#[test]
fn a_snapshot_keeps_its_refs_roles_and_flags() {
    let snapshot = parse_snapshot(
        r#"{"ok":true,"url":"https://example.com/login","title":"Sign in",
            "controls":[
              {"ref":1,"role":"heading","name":"Sign in","value":null,"focused":false,"disabled":false,"checked":null},
              {"ref":2,"role":"textbox","name":"Email","value":"ada@example.com","focused":true,"disabled":false,"checked":null},
              {"ref":3,"role":"checkbox","name":"Remember me","value":null,"focused":false,"disabled":false,"checked":true}
            ],
            "text":"Sign in to continue.","truncated":false}"#,
    )
    .expect("a snapshot");
    assert_eq!(snapshot.url, "https://example.com/login");
    assert_eq!(snapshot.title, "Sign in");
    assert_eq!(snapshot.controls.len(), 3);
    assert_eq!(snapshot.controls[0].role, "heading");
    assert_eq!(snapshot.controls[1].ref_id, 2);
    assert_eq!(
        snapshot.controls[1].value.as_deref(),
        Some("ada@example.com")
    );
    assert!(snapshot.controls[1].focused);
    assert_eq!(snapshot.controls[2].checked, Some(true));
    assert!(!snapshot.truncated);
}

#[test]
fn a_cut_snapshot_says_so() {
    let snapshot = parse_snapshot(
        r#"{"ok":true,"url":"https://example.com/","title":"",
            "controls":[],
            "text":"\u2026 12 more controls not shown","truncated":true}"#,
    )
    .expect("a snapshot");
    assert!(snapshot.truncated);
    assert!(snapshot.text.contains("12 more controls not shown"));
    assert!(snapshot.title.is_empty());
}

#[test]
fn a_password_value_never_survives_the_parser() {
    let snapshot = parse_snapshot(
        r#"{"ok":true,"url":"https://example.com/","title":"",
            "controls":[
              {"ref":1,"role":"password","name":"Password","value":"hunter2","focused":false,"disabled":false,"checked":null},
              {"ref":2,"role":"textbox","name":"Email","value":"ada@example.com","focused":false,"disabled":false,"checked":null}
            ],
            "text":"","truncated":false}"#,
    )
    .expect("a snapshot");
    assert_eq!(snapshot.controls[0].value, None);
    assert_eq!(
        snapshot.controls[1].value.as_deref(),
        Some("ada@example.com")
    );
}

#[test]
fn a_role_cannot_make_a_password_readable() {
    // `<input type="password" role="textbox" value="hunter2">`: the type, not the role,
    // makes it secret, and the host drops the value even if the page sends one.
    let snapshot = parse_snapshot(
        r#"{"ok":true,"url":"https://example.com/","title":"",
            "controls":[
              {"ref":1,"role":"textbox","name":"Password","value":"hunter2","secret":true,"focused":false,"disabled":false,"checked":null}
            ],
            "text":"","truncated":false}"#,
    )
    .expect("a snapshot");
    assert_eq!(snapshot.controls[0].role, "textbox");
    assert_eq!(snapshot.controls[0].value, None);
}

#[test]
fn malformed_json_is_refused() {
    assert_eq!(
        parse_snapshot("{not json"),
        Err(OpError::Failed(
            "the page could not be read; take the page over or try again".into()
        ))
    );
}

#[test]
fn an_empty_reply_is_refused() {
    assert_eq!(
        parse_snapshot(""),
        Err(OpError::Failed(
            "the page could not be read; take the page over or try again".into()
        ))
    );
    assert!(parse_info("   ").is_err());
}

#[test]
fn a_refused_script_keeps_its_own_error() {
    assert_eq!(
        parse_snapshot(r#"{"ok":false,"code":"not-found","message":"ref 7 is not on the page"}"#),
        Err(OpError::NotFound("ref 7 is not on the page".into()))
    );
}
