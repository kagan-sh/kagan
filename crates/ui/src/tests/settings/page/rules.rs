use gpui_kit::{AnyWindowHandle, TestAppContext};
use kg_core::Workspace;
use kg_domain::{InstructionsOp, Op};

use super::aria_label;
use crate::tests::kagan::{click, disabled, open, present, value_of, wait_until};

fn open_general(cx: &mut TestAppContext, window: AnyWindowHandle) {
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-general");
}

#[gpui_kit::test]
fn general_has_an_agent_rules_row_with_its_note(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    open_general(cx, window);
    assert_eq!(
        aria_label(cx, window, "setting-row-agent-rules").as_deref(),
        Some("Agent rules")
    );
    assert_eq!(
        aria_label(cx, window, "setting-row-agent-rules-note").as_deref(),
        Some(
            "Text each agent is told when it starts. Projects add their own from the project menu."
        )
    );
}

#[gpui_kit::test]
fn agent_rules_is_one_row_in_the_agents_group(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    open_general(cx, window);
    assert!(present(cx, window, "setting-row-agent-integration"));
    assert!(present(cx, window, "setting-row-agent-rules"));
    assert!(!present(cx, window, "settings-group-agent-rules"));
}

#[gpui_kit::test]
fn a_hand_edited_file_over_the_cap_disables_save_and_says_why(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    std::fs::write(data.path().join("instructions.md"), "a".repeat(20 * 1024)).unwrap();
    open_general(cx, window);
    wait_until(cx, |cx| {
        aria_label(cx, window, "agent-rules-status")
            .is_some_and(|status| status.contains("first 16,384 bytes"))
    });
    assert_eq!(disabled(cx, window, "agent-rules-save"), Some(true));
}

#[gpui_kit::test]
fn a_draft_survives_leaving_and_reopening_settings(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    open_general(cx, window);
    click(cx, window, "agent-rules-text");
    cx.simulate_input(window, "draft rule");
    cx.run_until_parked();
    click(cx, window, "settings-gear");
    open_general(cx, window);
    cx.run_until_parked();
    assert!(
        value_of(cx, window, "agent-rules-text")
            .unwrap_or_default()
            .contains("draft rule")
    );
}

fn set_rules_elsewhere(cx: &mut TestAppContext, text: &str) {
    let set = cx.update(|cx| {
        Workspace::dispatch(
            Op::Instructions(InstructionsOp::Set {
                text: Some(text.into()),
                file: None,
                project: None,
                expected: None,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(set).unwrap();
}

#[gpui_kit::test]
fn the_rules_box_follows_a_change_made_elsewhere_while_settings_is_open(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    open_general(cx, window);
    set_rules_elsewhere(cx, "rules from cli");
    wait_until(cx, |cx| {
        value_of(cx, window, "agent-rules-text").is_some_and(|text| text.contains("rules from cli"))
    });
}

#[gpui_kit::test]
fn save_refuses_rules_changed_elsewhere_and_keeps_the_draft(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    set_rules_elsewhere(cx, "first");
    open_general(cx, window);
    wait_until(cx, |cx| {
        value_of(cx, window, "agent-rules-text").is_some_and(|text| text.contains("first"))
    });
    click(cx, window, "agent-rules-text");
    cx.simulate_input(window, " draft");
    cx.run_until_parked();
    set_rules_elsewhere(cx, "second");
    wait_until(cx, |cx| {
        aria_label(cx, window, "agent-rules-notice").is_some()
    });
    cx.simulate_input(window, "!");
    click(cx, window, "agent-rules-save");
    wait_until(cx, |cx| {
        aria_label(cx, window, "agent-rules-notice").is_some()
    });
    let stored = std::fs::read_to_string(data.path().join("instructions.md")).unwrap();
    assert_eq!(
        stored, "second",
        "the first Save must not replace the newer rules"
    );

    click(cx, window, "agent-rules-save");
    wait_until(cx, |cx| {
        std::fs::read_to_string(data.path().join("instructions.md")).unwrap() == "first draft!"
    });
}
