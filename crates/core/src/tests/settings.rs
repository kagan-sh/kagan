use gpui_kit::TestAppContext;
use kg_domain::*;
use serde_json::json;

use super::{Fixture, op};
use crate::Workspace;

#[gpui_kit::test]
async fn preference_set_stores_a_boolean(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let refused = op(
        Op::Preference(PreferenceOp::Set {
            key: "confirm-archive".into(),
            value: json!("yes"),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(refused, OpError::Invalid(ref message) if message.contains("boolean")),
        "{refused}"
    );
    op(
        Op::Preference(PreferenceOp::Set {
            key: "confirm-archive".into(),
            value: json!(true),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Setting(value) = op(
        Op::Preference(PreferenceOp::Get {
            key: "confirm-archive".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("preference")
    };
    assert_eq!(value, json!(true));
}

#[gpui_kit::test]
async fn the_preference_keys_named_in_help_are_accepted(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for key in [
        "confirm-archive",
        "agent-integration",
        "notify",
        "plugin-status",
        "terminal-font-size",
        "branch-prefix",
        "terminal-copy-on-select",
        "terminal-links-browser",
        "density",
        "main.left-sidebar",
        "main.right-sidebar",
        "main.left-width",
        "main.right-width",
        "main.review-wrap",
    ] {
        let result = op(Op::Preference(PreferenceOp::Get { key: key.into() }), cx).await;
        assert!(result.is_ok(), "help names {key}, which is refused");
    }
}

#[gpui_kit::test]
async fn plugin_status_is_a_boolean(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let refused = op(
        Op::Preference(PreferenceOp::Set {
            key: "plugin-status".into(),
            value: json!("yes"),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(refused, OpError::Invalid(ref message) if message.contains("boolean")),
        "{refused}"
    );
    op(
        Op::Preference(PreferenceOp::Set {
            key: "plugin-status".into(),
            value: json!(false),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Setting(value) = op(
        Op::Preference(PreferenceOp::Get {
            key: "plugin-status".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("preference")
    };
    assert_eq!(value, json!(false));
}

#[gpui_kit::test]
async fn terminal_font_size_accepts_only_whole_points_in_range(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for value in [json!(8), json!(25), json!(12.5), json!("12")] {
        let result = op(
            Op::Preference(PreferenceOp::Set {
                key: "terminal-font-size".into(),
                value: value.clone(),
            }),
            cx,
        )
        .await;
        assert!(
            matches!(result, Err(OpError::Invalid(_))),
            "accepted {value}"
        );
    }
    for value in [9, 24] {
        op(
            Op::Preference(PreferenceOp::Set {
                key: "terminal-font-size".into(),
                value: json!(value),
            }),
            cx,
        )
        .await
        .unwrap();
    }
}

#[gpui_kit::test]
async fn a_theme_set_after_the_migration_stays(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(
        Op::Theme(ThemeOp::Set {
            theme: "ember".into(),
            mode: Appearance::Dark,
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Theme(settings) = op(Op::Theme(ThemeOp::Get), cx).await.unwrap() else {
        panic!("theme")
    };
    assert_eq!(settings.theme, "ember");
}

#[gpui_kit::test]
async fn a_saved_shortcut_is_kept_and_announced(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let announced = std::rc::Rc::new(std::cell::Cell::new(false));
    let workspace = fixture.workspace.clone();
    let _subscription = cx.update({
        let announced = announced.clone();
        move |cx| {
            cx.subscribe(&workspace, move |_, event: &crate::WorkspaceEvent, _| {
                if matches!(event, crate::WorkspaceEvent::Changed) {
                    announced.set(true);
                }
            })
        }
    });
    op(
        Op::Keymap(kg_domain::KeymapOp::Set {
            action: "NewThread".into(),
            key: "secondary-j".into(),
            reset: false,
        }),
        cx,
    )
    .await
    .unwrap();
    cx.run_until_parked();
    assert!(
        announced.get(),
        "the window was not told of the saved shortcut"
    );
    let saved = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .keymap()
            .binding("NewThread")
            .map(str::to_owned)
    });
    let expected = crate::parse_key("secondary-j")
        .expect("secondary-j parses")
        .unparse(crate::host_os());
    assert_eq!(saved.as_deref(), Some(expected.as_str()));
}

#[gpui_kit::test]
async fn shortcut_keys_are_checked_before_they_are_saved(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for key in ["$(rm)", "cmd-🚀", "f01", "f+1", "f0", "f36", "cmd-f1x"] {
        let result = op(
            Op::Keymap(kg_domain::KeymapOp::Set {
                action: "NewThread".into(),
                key: key.into(),
                reset: false,
            }),
            cx,
        )
        .await;
        assert!(result.is_err(), "accepted {key}");
    }
    for key in [
        "secondary-alt-n",
        "cmd-shift-k",
        "cmd-,",
        "f1",
        "f35",
        "cmd-ä",
    ] {
        let result = op(
            Op::Keymap(kg_domain::KeymapOp::Set {
                action: "NewThread".into(),
                key: key.into(),
                reset: false,
            }),
            cx,
        )
        .await;
        assert!(result.is_ok(), "refused {key}");
    }
}

#[gpui_kit::test]
async fn every_default_shortcut_passes_the_key_check(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for shortcut in kg_domain::SHORTCUTS {
        let result = op(
            Op::Keymap(kg_domain::KeymapOp::Set {
                action: shortcut.name.into(),
                key: shortcut.chord.into(),
                reset: false,
            }),
            cx,
        )
        .await;
        assert!(result.is_ok(), "default {} is refused", shortcut.chord);
    }
}

#[gpui_kit::test]
async fn notify_is_all_asks_or_off(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for refused in [json!(true), json!(false), json!("yes")] {
        let error = op(
            Op::Preference(PreferenceOp::Set {
                key: "notify".into(),
                value: refused,
            }),
            cx,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, OpError::Invalid(ref message) if message.contains("all, asks or off")),
            "{error}"
        );
    }
    for level in ["all", "asks", "off"] {
        op(
            Op::Preference(PreferenceOp::Set {
                key: "notify".into(),
                value: json!(level),
            }),
            cx,
        )
        .await
        .unwrap();
    }
}

#[gpui_kit::test]
async fn branch_prefix_must_start_a_git_branch(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for good in ["", "bb/", "feat-", "a-b/c-"] {
        op(
            Op::Preference(PreferenceOp::Set {
                key: "branch-prefix".into(),
                value: json!(good),
            }),
            cx,
        )
        .await
        .unwrap_or_else(|error| panic!("{good:?} is refused: {error}"));
    }
    for bad in [
        "bb",
        "feat_",
        "refs/x/",
        "bad space/",
        "-flag",
        "a..b",
        "x~",
        "a/b.lock/",
        &"x".repeat(41),
    ] {
        let refused = op(
            Op::Preference(PreferenceOp::Set {
                key: "branch-prefix".into(),
                value: json!(bad),
            }),
            cx,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(refused, OpError::Invalid(_)),
            "{bad:?} gave {refused}"
        );
    }
    let OpResult::Setting(value) = op(
        Op::Preference(PreferenceOp::Get {
            key: "branch-prefix".into(),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("preference")
    };
    assert_eq!(value, json!("a-b/c-"));
}

#[gpui_kit::test]
async fn a_built_in_shortcut_can_lose_its_key_and_reset_brings_it_back(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(
        Op::Keymap(KeymapOp::Remove {
            action: "NewThread".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    cx.run_until_parked();
    let unbound = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .keymap()
            .binding("NewThread")
            .map(str::to_owned)
    });
    assert_eq!(unbound.as_deref(), Some(""));
    op(
        Op::Keymap(KeymapOp::Set {
            action: "NewThread".into(),
            key: "secondary-j".into(),
            reset: false,
        }),
        cx,
    )
    .await
    .unwrap();
    op(
        Op::Keymap(KeymapOp::Set {
            action: "NewThread".into(),
            key: String::new(),
            reset: true,
        }),
        cx,
    )
    .await
    .unwrap();
    cx.run_until_parked();
    let reset = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .keymap()
            .binding("NewThread")
            .map(str::to_owned)
    });
    assert_eq!(reset, None);
}

#[gpui_kit::test]
async fn font_names_are_family_names_or_empty(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    for good in ["", "Helvetica Neue", "JetBrains Mono"] {
        op(
            Op::Preference(PreferenceOp::Set {
                key: "font-ui".into(),
                value: json!(good),
            }),
            cx,
        )
        .await
        .unwrap_or_else(|error| panic!("{good:?} is refused: {error}"));
    }
    for bad in ["bad\tname", &"x".repeat(81)] {
        let refused = op(
            Op::Preference(PreferenceOp::Set {
                key: "font-terminal".into(),
                value: json!(bad),
            }),
            cx,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(refused, OpError::Invalid(_)),
            "{bad:?} gave {refused}"
        );
    }
}
