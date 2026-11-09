use super::{Fixture, op};
use gpui_kit::TestAppContext;
use kg_domain::{Op, OpError, OpResult, Skill, SkillOp};
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
async fn bb_folders_are_not_read(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("bb/skills"), "bb-only");
    write_skill(&fixture.dir.path().join(".bb/skills"), "bb-project");
    let skills = listed(&fixture, cx).await;
    assert!(
        skills.iter().all(|skill| !skill.name.starts_with("bb-")),
        "{skills:?}"
    );
    let text = "---\nname: bb-only\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "bb-only", text, cx).await,
        Err(OpError::NotFound(_))
    ));
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

#[gpui_kit::test]
async fn open_is_accepted_without_a_window(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    assert!(matches!(
        op(Op::Skill(SkillOp::Open), cx).await,
        Ok(OpResult::Done)
    ));
    let _ = fixture;
}
