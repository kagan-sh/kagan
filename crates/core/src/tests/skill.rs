use super::{Fixture, op};
use gpui_kit::TestAppContext;
use kg_domain::{Op, OpError, OpResult, Skill, SkillOp};
use std::path::Path;

#[cfg(unix)]
use super::install_plugin;

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
    write_skill(&fixture.dir.path().join(".bb/skills"), "bb-project");
    let skills = listed(&fixture, cx).await;
    assert!(
        skills.iter().all(|skill| !skill.name.starts_with("bb-")),
        "{skills:?}"
    );
    let text = "---\nname: bb-project\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "bb-project", text, cx).await,
        Err(OpError::NotFound(_))
    ));
    assert!(
        fixture
            .dir
            .path()
            .join(".bb/skills/bb-project/SKILL.md")
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
            expected: None,
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
#[cfg(unix)]
#[gpui_kit::test]
async fn update_refuses_a_plugin_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    install_plugin(fixture.dir.path(), true);
    let text = "---\nname: demo\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "demo", text, cx).await,
        Err(OpError::Invalid(_))
    ));
    let written =
        std::fs::read_to_string(fixture.dir.path().join("demo-plugin/skills/demo/SKILL.md"))
            .unwrap();
    assert!(written.contains("A demo skill."), "{written}");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn remove_refuses_a_plugin_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    install_plugin(fixture.dir.path(), true);
    let removed = op(
        Op::Skill(SkillOp::Remove {
            name: "demo".into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await;
    assert!(matches!(removed, Err(OpError::Invalid(_))));
    assert!(
        fixture
            .dir
            .path()
            .join("demo-plugin/skills/demo/SKILL.md")
            .exists()
    );
}

#[gpui_kit::test]
async fn update_refuses_a_file_changed_since_it_was_read(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    write_skill(&fixture.dir.path().join("skills"), "mine");
    let path = fixture.dir.path().join("skills/mine/SKILL.md");
    let before = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, before.replace("A test skill.", "Changed elsewhere.")).unwrap();
    let text = "---\nname: mine\ndescription: Edited.\n---\n\nNew body.\n";
    let refused = op(
        Op::Skill(SkillOp::Update {
            name: "mine".into(),
            text: text.into(),
            expected: Some(before),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await;
    assert!(matches!(refused, Err(OpError::Invalid(_))));
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("Changed elsewhere.")
    );
    let current = std::fs::read_to_string(&path).unwrap();
    let saved = op(
        Op::Skill(SkillOp::Update {
            name: "mine".into(),
            text: text.into(),
            expected: Some(current),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await;
    assert!(matches!(saved, Ok(OpResult::Done)));
    assert_eq!(std::fs::read_to_string(&path).unwrap(), text);
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
async fn open_asks_the_window_for_the_skills_page(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let asked = std::rc::Rc::new(std::cell::Cell::new(0));
    let _asks = cx.update(|cx| {
        let asked = asked.clone();
        cx.subscribe(&fixture.workspace, move |_, event, _| {
            if matches!(event, crate::WorkspaceEvent::Skills) {
                asked.set(asked.get() + 1);
            }
        })
    });
    assert!(matches!(
        op(Op::Skill(SkillOp::Open), cx).await,
        Ok(OpResult::Done)
    ));
    cx.run_until_parked();
    assert_eq!(asked.get(), 1);
}
