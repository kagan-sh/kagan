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

/// A question the agent asked through `threads.ask` carries its fields, even when empty; a
/// permission request does not, and an approval is one.
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
