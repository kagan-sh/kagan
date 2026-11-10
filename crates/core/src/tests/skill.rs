use super::{Fixture, op};
use crate::PluginRequest;
use gpui_kit::TestAppContext;
use kg_domain::{Op, OpError, OpResult, PluginCommand, PluginId, Skill, SkillOp};
use std::path::Path;
use std::rc::Rc;

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
async fn a_bundled_skill_lists_files_that_exist_on_disk(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Files(files) = op(
        Op::Skill(SkillOp::Files {
            name: "kg-cli".into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("files")
    };
    assert_eq!(files.len(), 1, "{files:?}");
    assert!(files[0].ends_with("SKILL.md"), "{files:?}");
    assert_eq!(
        std::fs::read_to_string(&files[0]).unwrap(),
        include_str!("../../skills/kg-cli/SKILL.md")
    );
}

#[gpui_kit::test]
async fn plugin_commands_become_a_generated_skill(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    assert!(
        listed(&fixture, cx)
            .await
            .iter()
            .all(|skill| skill.name != "kg-plugin-commands")
    );
    fixture.workspace.update(cx, |workspace, _| {
        workspace.plugins = Some(Rc::new(
            |request: PluginRequest, _cx: &mut gpui_kit::App| {
                let result = match request {
                    PluginRequest::Commands => OpResult::PluginCommands(vec![PluginCommand {
                        plugin: PluginId("dev.test.probe".into()),
                        group: "probe".into(),
                        command: "hang".into(),
                        usage: Some("hang <seconds>".into()),
                    }]),
                    _ => OpResult::Warnings(Vec::new()),
                };
                gpui_kit::Task::ready(Ok(result))
            },
        ));
    });
    let OpResult::Skill { body, .. } = op(
        Op::Skill(SkillOp::Show {
            name: "kg-plugin-commands".into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("skill")
    };
    assert!(
        body.contains("- `kg probe hang <seconds>`, from dev.test.probe"),
        "{body}"
    );
}

#[gpui_kit::test]
async fn a_split_skill_lists_the_reference_files_it_names(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let OpResult::Files(files) = op(
        Op::Skill(SkillOp::Files {
            name: "kg-plugin-port".into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("files")
    };
    assert!(
        files
            .iter()
            .any(|file| file.ends_with("references/mapping.md")),
        "{files:?}"
    );
    assert!(files.iter().all(|file| file.exists()), "{files:?}");
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

#[cfg(unix)]
fn link_skill(fixture: &Fixture) -> std::path::PathBuf {
    write_skill(&fixture.dir.path().join("elsewhere"), "mine");
    std::fs::create_dir_all(fixture.dir.path().join("skills")).unwrap();
    let link = fixture.dir.path().join("skills/mine");
    std::os::unix::fs::symlink(fixture.dir.path().join("elsewhere/mine"), &link).unwrap();
    link
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_linked_skill_folder_is_not_listed_or_touched(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let link = link_skill(&fixture);
    assert!(
        listed(&fixture, cx)
            .await
            .iter()
            .all(|skill| skill.name != "mine")
    );
    let text = "---\nname: mine\ndescription: Edited.\n---\n\nNew body.\n";
    assert!(matches!(
        update(&fixture, "mine", text, cx).await,
        Err(OpError::NotFound(_))
    ));
    let removed = op(
        Op::Skill(SkillOp::Remove {
            name: "mine".into(),
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await;
    assert!(matches!(removed, Err(OpError::NotFound(_))));
    assert!(std::fs::symlink_metadata(&link).is_ok());
    assert!(
        std::fs::read_to_string(fixture.dir.path().join("elsewhere/mine/SKILL.md"))
            .unwrap()
            .contains("A test skill.")
    );
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
