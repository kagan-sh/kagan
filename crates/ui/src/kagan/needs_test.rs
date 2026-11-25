use super::*;

fn option(id: &str, role: Option<OptionRole>) -> InteractionOption {
    InteractionOption {
        id: id.into(),
        label: id.into(),
        role,
    }
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
