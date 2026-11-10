use gpui_kit::TestAppContext;

use super::aria_label;
use crate::tests::kagan::{click, open};

#[gpui_kit::test]
fn general_has_an_agent_rules_row_with_its_note(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    click(cx, window, "settings-gear");
    click(cx, window, "settings-section-general");
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
