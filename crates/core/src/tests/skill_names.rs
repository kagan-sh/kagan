use gpui_kit::TestAppContext;
use kg_domain::{Op, SkillOp};

use super::{Fixture, op};

#[gpui_kit::test]
async fn a_new_skill_needs_a_plain_name_outside_the_kg_prefix(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let new = |name: &str| {
        Op::Skill(SkillOp::New {
            name: name.into(),
            project: true,
            project_id: Some(fixture.project.clone()),
        })
    };
    for name in ["kg-helper", "Bad Name", "", "a--b"] {
        let error = op(new(name), cx).await.unwrap_err();
        assert!(
            error.to_string().contains("lowercase and hyphenated"),
            "{name}: {error}"
        );
    }
    op(new("good-skill"), cx).await.unwrap();
}
