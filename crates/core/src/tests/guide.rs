use super::{Fixture, op};
use gpui_kit::TestAppContext;
use kg_domain::*;

#[gpui_kit::test]
async fn the_guide_contains_its_map(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let OpResult::Text(guide) = op(Op::Guide { chapter: None }, cx).await.unwrap() else {
        panic!("guide")
    };
    assert!(guide.contains(include_str!("../guide-map.md")));
}

#[gpui_kit::test]
async fn the_customize_chapter_names_each_place_to_change(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    let OpResult::Text(map) = op(Op::Guide { chapter: None }, cx).await.unwrap() else {
        panic!("guide")
    };
    assert!(map.contains("customize"), "{map}");
    let OpResult::Text(text) = op(
        Op::Guide {
            chapter: Some(GuideChapter::Customize),
        },
        cx,
    )
    .await
    .unwrap() else {
        panic!("guide")
    };
    for part in [
        "<dataDir>/themes/<id>.json",
        "kg theme new",
        "kg keymap set",
        "kg preference set",
        "notify",
        "slot.review",
        "instructions.md",
        "4096",
        "kg skill new",
    ] {
        assert!(text.contains(part), "missing {part}: {text}");
    }
}

#[test]
fn the_customize_skill_points_to_the_chapter() {
    let skill = include_str!("../../skills/kg-customize/SKILL.md");
    assert!(skill.contains("kg guide customize"), "{skill}");
}

#[gpui_kit::test]
async fn the_customize_chapter_says_which_agents_get_instructions_and_how_to_reset_a_key(
    cx: &mut TestAppContext,
) {
    let _fixture = Fixture::new(cx).await;
    let OpResult::Text(text) = op(
        Op::Guide {
            chapter: Some(GuideChapter::Customize),
        },
        cx,
    )
    .await
    .unwrap() else {
        panic!("guide")
    };
    assert!(
        text.contains("`claude`, `codex`, `cursor-agent` and `pi`"),
        "{text}"
    );
    assert!(
        text.contains("`kimi` and `opencode` do not get it"),
        "{text}"
    );
    assert!(
        text.contains(r#"kg keymap set <action> "" --reset"#),
        "{text}"
    );
}
