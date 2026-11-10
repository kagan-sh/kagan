use super::{Kind, LAYOUT, PREFERENCES, kind, known_preference, preference_value};

#[test]
fn each_window_layout_key_has_its_own_kind() {
    use kg_domain::prefs;
    assert_eq!(
        kind(&format!("main.{}", prefs::LEFT_SIDEBAR)),
        Some(Kind::Flag)
    );
    assert_eq!(
        kind(&format!("main.{}", prefs::LEFT_WIDTH)),
        Some(Kind::Pixels)
    );
    assert_eq!(
        kind(&format!("main.{}", prefs::RIGHT_SIDEBAR)),
        Some(Kind::Flag)
    );
    assert_eq!(
        kind(&format!("main.{}", prefs::RIGHT_WIDTH)),
        Some(Kind::Pixels)
    );
    assert_eq!(
        kind(&format!("main.{}", prefs::REVIEW_WRAP)),
        Some(Kind::Flag)
    );
}

#[test]
fn only_a_known_preference_passes_the_check() {
    use kg_domain::prefs;
    assert!(known_preference(&format!("main.{}", prefs::LEFT_WIDTH)).is_ok());
    assert!(known_preference("main.not-a-setting").is_err());
}

#[test]
fn guide_names_every_preference() {
    let guide = include_str!("../../../../docs/customize.md");
    let behaviour = include_str!("../../../../docs/behaviour.md");
    let command =
        <kg_domain::Op as clap::Subcommand>::augment_subcommands(clap::Command::new("kg"));
    let help = command
        .find_subcommand("preference")
        .expect("preference command")
        .get_about()
        .expect("preference help")
        .to_string();
    for key in PREFERENCES.iter().map(|(name, _)| *name) {
        assert!(guide.contains(&format!("`{key}`")), "guide missing {key}");
        assert!(
            behaviour.contains(&format!("`{key}`")),
            "behaviour missing {key}"
        );
        assert!(help.contains(key), "help missing {key}");
    }
    for key in ["slot.threads", "slot.review"] {
        assert!(guide.contains(&format!("`{key}`")), "guide missing {key}");
    }
    for key in LAYOUT.iter().map(|(name, _)| *name) {
        assert!(
            guide.contains(&format!("`<window>.{key}`")),
            "guide missing {key}"
        );
        assert!(
            behaviour.contains(&format!("`<window>.{key}`")),
            "behaviour missing {key}"
        );
        assert!(
            help.contains(&format!("<window>.{key}")),
            "help missing {key}"
        );
    }
}

#[test]
fn sidebar_list_options_have_a_fixed_shape() {
    use kg_domain::prefs;
    assert!(preference_value(prefs::SIDEBAR_ORDER, &serde_json::json!("current")).is_ok());
    assert!(preference_value(prefs::SIDEBAR_ORDER, &serde_json::json!("attention")).is_ok());
    assert!(preference_value(prefs::SIDEBAR_ORDER, &serde_json::json!("newest")).is_err());
    assert!(preference_value(prefs::SIDEBAR_HIDE_EMPTY_PROJECTS, &serde_json::json!(true)).is_ok());
    assert!(
        preference_value(
            prefs::SIDEBAR_HIDE_EMPTY_SECTIONS,
            &serde_json::json!("yes")
        )
        .is_err()
    );
    assert!(preference_value(prefs::SIDEBAR_UNREAD_FIRST, &serde_json::json!(false)).is_ok());
}

#[test]
fn review_wrap_is_a_window_boolean() {
    assert!(known_preference("main.review-wrap").is_ok());
    assert!(preference_value("main.review-wrap", &serde_json::json!(true)).is_ok());
    assert!(preference_value("main.review-wrap", &serde_json::json!("true")).is_err());
}
