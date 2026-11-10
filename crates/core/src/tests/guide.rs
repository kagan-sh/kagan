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
    assert!(text.contains("off by default"));
    assert!(text.contains("a plugin id, or `kagan`"));
    let cap = crate::thread::agent::note::INSTRUCTIONS_CAP.to_string();
    assert!(text.contains(&cap));
    assert!(text.contains(crate::thread::agent::note::INSTRUCTIONS_PREFIX));
}

#[test]
fn every_kg_command_the_customize_chapter_names_exists() {
    let text = include_str!("../../../../docs/customize.md");
    let root = <kg_domain::Op as clap::Subcommand>::augment_subcommands(clap::Command::new("kg"));
    let mut checked = 0;
    for span in text.split('`').skip(1).step_by(2) {
        let Some(rest) = span.strip_prefix("kg ") else {
            continue;
        };
        let words = rest
            .split_whitespace()
            .take_while(|word| word.bytes().all(|b| b.is_ascii_lowercase() || b == b'-'))
            .take(2);
        let mut command = &root;
        for word in words {
            if !command.has_subcommands() {
                break;
            }
            command = command
                .find_subcommand(word)
                .unwrap_or_else(|| panic!("`kg {word}` is not a command, in `{span}`"));
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} commands were checked");
}
