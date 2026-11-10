use std::path::Path;

use gpui_kit::TestAppContext;
use kg_domain::{Op, OpResult, PluginId, PluginOp, Skill, SkillOp, SkillSource};
use serde_json::json;

use super::{
    ID, answer_standalone, install, installed, open, plugin_dir, plugin_op, records, run,
    sample_manifest, write_plugin,
};

fn with_skill(dir: &Path, name: &str) {
    let folder = dir.join("skills").join(name);
    std::fs::create_dir_all(&folder).unwrap();
    std::fs::write(
        folder.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: A test skill.\n---\n\nDo it.\n"),
    )
    .unwrap();
}

fn declare_skills(dir: &Path, names: &[&str]) {
    let mut manifest = sample_manifest();
    manifest["skills"] = json!(
        names
            .iter()
            .map(|name| format!("skills/{name}"))
            .collect::<Vec<_>>()
    );
    std::fs::write(dir.join(crate::manifest::FILE), manifest.to_string()).unwrap();
}

fn plugin_skills(cx: &mut TestAppContext) -> Vec<Skill> {
    let OpResult::Skills { skills, .. } =
        run(cx, Op::Skill(SkillOp::List { project_id: None })).unwrap()
    else {
        panic!("skills")
    };
    skills
        .into_iter()
        .filter(|skill| skill.source == SkillSource::Plugin)
        .collect()
}

fn names(skills: &[Skill]) -> Vec<&str> {
    skills.iter().map(|skill| skill.name.as_str()).collect()
}

#[gpui_kit::test]
fn an_installed_plugin_skill_is_listed_under_the_plugin_folder(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    with_skill(plugin.path(), "demo");
    declare_skills(plugin.path(), &["demo"]);
    open(cx, &[]);
    let installing = plugin_op(cx, install(plugin.path().display().to_string(), None, None));
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(installing).unwrap();
    let record = records(cx).remove(0);
    assert_eq!(record.skills, vec![Path::new("skills/demo").to_path_buf()]);
    let skills = plugin_skills(cx);
    assert_eq!(names(&skills), ["demo"]);
    let root = dunce::canonicalize(&record.root).unwrap();
    assert!(skills[0].path.as_ref().unwrap().starts_with(root));
}

#[gpui_kit::test]
fn a_record_with_no_skill_list_gets_one_at_startup(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    with_skill(plugin.path(), "demo");
    declare_skills(plugin.path(), &["demo"]);
    open(cx, &[installed(plugin.path())]);
    assert_eq!(
        records(cx).remove(0).skills,
        vec![Path::new("skills/demo").to_path_buf()]
    );
    assert_eq!(names(&plugin_skills(cx)), ["demo"]);
}

#[gpui_kit::test]
fn reload_replaces_the_plugin_skill_list(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    with_skill(plugin.path(), "demo");
    declare_skills(plugin.path(), &["demo"]);
    open(cx, &[installed(plugin.path())]);
    assert_eq!(names(&plugin_skills(cx)), ["demo"]);
    with_skill(plugin.path(), "next");
    declare_skills(plugin.path(), &["next"]);
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    cx.foreground_executor().block_test(reloading).unwrap();
    assert_eq!(
        records(cx).remove(0).skills,
        vec![Path::new("skills/next").to_path_buf()]
    );
    assert_eq!(names(&plugin_skills(cx)), ["next"]);
}

#[gpui_kit::test]
fn a_running_plugins_cli_commands_appear_in_the_generated_skill(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let mut manifest = sample_manifest();
    manifest["cli"] =
        json!([{ "name": "probe", "commands": [{ "name": "hang", "usage": "hang <seconds>" }] }]);
    write_plugin(dir.path(), &manifest);
    open(cx, &[installed(dir.path())]);
    cx.run_until_parked();
    let OpResult::Skill { body, .. } = run(
        cx,
        Op::Skill(SkillOp::Show {
            name: "kg-plugin-commands".into(),
            project_id: None,
        }),
    )
    .unwrap() else {
        panic!("skill")
    };
    assert!(
        body.contains(&format!("- `kg probe hang <seconds>`, from {ID}")),
        "{body}"
    );
}
