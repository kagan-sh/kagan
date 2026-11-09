use super::{Fixture, op};
use gpui_kit::TestAppContext;
use kg_domain::{Op, OpError, OpResult, Skill, SkillOp, SkillSource};
use std::path::Path;

fn write_skill(root: &Path, name: &str) {
    let folder = root.join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A test skill.\n---\n\nDo it.\n"),
    )
    .unwrap();
}

async fn listed(fixture: &Fixture, cx: &mut TestAppContext) -> Vec<Skill> {
    let OpResult::Skills { skills, .. } = op(
        Op::Skill(SkillOp::List {
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("skills")
    };
    skills
}

#[gpui_kit::test]
async fn list_includes_the_bb_skills_as_external(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("bb/skills"), "bb-only");
    write_skill(&fixture.dir.path().join(".bb/skills"), "bb-project");
    let skills = listed(&fixture, cx).await;
    let external: Vec<_> = skills
        .iter()
        .filter(|skill| skill.source == SkillSource::External)
        .map(|skill| skill.name.as_str())
        .collect();
    assert_eq!(external, ["bb-only", "bb-project"]);
}

#[gpui_kit::test]
async fn remove_refuses_a_bb_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("bb/skills"), "bb-only");
    let result = op(
        Op::Skill(SkillOp::Remove {
            name: "bb-only".into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await;
    assert!(matches!(result, Err(OpError::Invalid(_))), "{result:?}");
    assert!(
        fixture
            .dir
            .path()
            .join("bb/skills/bb-only/SKILL.md")
            .exists()
    );
}

async fn update(
    fixture: &Fixture,
    name: &str,
    text: &str,
    cx: &mut TestAppContext,
) -> Result<OpResult, OpError> {
    op(
        Op::Skill(SkillOp::Update {
            name: name.into(),
            text: text.into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
}

#[gpui_kit::test]
async fn update_replaces_a_user_skill_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("skills"), "mine");
    let text = "---\nname: mine\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "mine", text, cx).await,
        Ok(OpResult::Done)
    ));
    let written = std::fs::read_to_string(fixture.dir.path().join("skills/mine/SKILL.md")).unwrap();
    assert_eq!(written, text);
}

#[gpui_kit::test]
async fn update_refuses_a_bb_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("bb/skills"), "bb-only");
    let text = "---\nname: bb-only\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "bb-only", text, cx).await,
        Err(OpError::Invalid(_))
    ));
}

#[gpui_kit::test]
async fn update_refuses_a_bundled_kg_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let text = "---\nname: kg-cli\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "kg-cli", text, cx).await,
        Err(OpError::Invalid(_))
    ));
}

#[gpui_kit::test]
async fn update_refuses_text_with_another_name(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("skills"), "mine");
    let text = "---\nname: kg-stolen\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "mine", text, cx).await,
        Err(OpError::Invalid(_))
    ));
    assert!(
        std::fs::read_to_string(fixture.dir.path().join("skills/mine/SKILL.md"))
            .unwrap()
            .contains("name: mine")
    );
}
