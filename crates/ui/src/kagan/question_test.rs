use super::*;

fn option(id: &str, role: Option<OptionRole>) -> InteractionOption {
    InteractionOption {
        id: id.into(),
        label: id.into(),
        role,
    }
}

#[test]
fn a_permission_card_reads_deny_then_always_allow_then_allow_once() {
    let agent = vec![
        option("allow-once", Some(OptionRole::Primary)),
        option("allow-always", None),
        option("reject-once", Some(OptionRole::Destructive)),
    ];
    let ids: Vec<_> = ordered(&agent, true)
        .into_iter()
        .map(|option| option.id)
        .collect();
    assert_eq!(ids, ["reject-once", "allow-always", "allow-once"]);
}

#[test]
fn a_question_keeps_the_order_it_was_asked_in() {
    let agent = vec![
        option("allow-once", Some(OptionRole::Primary)),
        option("reject-once", Some(OptionRole::Destructive)),
    ];
    let ids: Vec<_> = ordered(&agent, false)
        .into_iter()
        .map(|option| option.id)
        .collect();
    assert_eq!(ids, ["allow-once", "reject-once"]);
}

#[test]
fn only_an_approval_or_a_question_without_fields_asks_permission() {
    let asked = |fields| PendingInteraction {
        id: PendingInteractionId("pn_1".into()),
        thread_id: ThreadId("thr".into()),
        title: "Which one?".into(),
        options: vec![option("left", None), option("right", None)],
        fields,
        created_at: kg_domain::Timestamp(1),
    };
    assert!(!Question::thread(&asked(Some(Vec::new()))).asks_permission());
    assert!(Question::thread(&asked(None)).asks_permission());
    let approval = Approval {
        id: PendingInteractionId("pn_2".into()),
        title: "Install this plugin?".into(),
        options: vec![option("install", Some(OptionRole::Primary))],
    };
    assert!(Question::standalone(&approval).asks_permission());
}

#[test]
fn a_permission_card_opens_on_the_option_that_grants_nothing() {
    let options = vec![
        option("allow-once", Some(OptionRole::Primary)),
        option("allow-always", None),
        option("reject-once", Some(OptionRole::Destructive)),
    ];
    assert_eq!(
        opening_option(&options, true).map(|option| option.id.as_str()),
        Some("reject-once"),
        "a permission card never opens on an allow"
    );
    assert!(
        opening_option(&[option("allow-once", Some(OptionRole::Primary))], true).is_none(),
        "with no way to refuse, the card opens on nothing"
    );
}

#[test]
fn a_question_opens_on_its_filled_option_else_on_its_first() {
    let asked = vec![
        option("left", None),
        option("right", Some(OptionRole::Destructive)),
    ];
    assert_eq!(
        opening_option(&asked, false).map(|option| option.id.as_str()),
        Some("left"),
        "a question keeps the order it was asked in"
    );
    let shaped = vec![
        option("reject-once", Some(OptionRole::Destructive)),
        option("allow-once", Some(OptionRole::Primary)),
    ];
    assert_eq!(
        opening_option(&shaped, false).map(|option| option.id.as_str()),
        Some("allow-once"),
        "a question opens on the option its asker filled"
    );
}
