use std::collections::BTreeMap;
use std::path::PathBuf;

use kg_domain::{Skill, SkillSource};

use super::catalog::{Catalog, Loaded, materialize, place, shared, valid_name, write};

const DEMO: &[(&str, &str, &str)] = &[
    (
        "kg-demo",
        "SKILL.md",
        "---\nname: kg-demo\ndescription: Demo.\n---\nBody.\n",
    ),
    ("kg-demo", "references/long.md", "Long text.\n"),
];

#[test]
fn placing_over_a_folder_another_reader_built_keeps_theirs() {
    let dir = tempfile::tempdir().unwrap();
    let stable = dir.path().join("stable");
    std::fs::create_dir_all(&stable).unwrap();
    std::fs::write(stable.join("SKILL.md"), "theirs").unwrap();
    let temp = dir.path().join("temp");
    std::fs::create_dir_all(&temp).unwrap();
    std::fs::write(temp.join("SKILL.md"), "mine").unwrap();
    place(&temp, &stable).unwrap();
    assert_eq!(
        std::fs::read_to_string(stable.join("SKILL.md")).unwrap(),
        "theirs"
    );
}

#[test]
fn a_bundled_folder_changed_on_disk_is_rebuilt_from_the_binary() {
    let dir = tempfile::tempdir().unwrap();
    let root = materialize(dir.path(), DEMO).unwrap();
    std::fs::write(root.join("kg-demo/SKILL.md"), "tampered").unwrap();
    let again = materialize(dir.path(), DEMO).unwrap();
    assert_eq!(
        std::fs::read_to_string(again.join("kg-demo/SKILL.md")).unwrap(),
        DEMO[0].2
    );
}

#[test]
fn a_bundled_folder_missing_a_file_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let root = materialize(dir.path(), DEMO).unwrap();
    std::fs::remove_file(root.join("kg-demo/references/long.md")).unwrap();
    let again = materialize(dir.path(), DEMO).unwrap();
    assert!(again.join("kg-demo/references/long.md").is_file());
}

#[test]
fn a_bundled_folder_with_a_stray_file_is_rebuilt() {
    let dir = tempfile::tempdir().unwrap();
    let root = materialize(dir.path(), DEMO).unwrap();
    std::fs::write(root.join("kg-demo/stray.md"), "extra").unwrap();
    let again = materialize(dir.path(), DEMO).unwrap();
    assert!(!again.join("kg-demo/stray.md").exists());
}

#[cfg(unix)]
#[test]
fn leftovers_older_than_an_hour_are_swept_and_fresh_ones_are_kept() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("agent/bundled");
    let old = std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 3600);
    let mut leftovers = Vec::new();
    for name in [".tmp-old", ".broken-old"] {
        let path = folder.join(name);
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(path.join("junk"), "x").unwrap();
        std::fs::File::open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        leftovers.push(path);
    }
    let fresh = folder.join(".tmp-fresh");
    std::fs::create_dir_all(&fresh).unwrap();
    materialize(dir.path(), DEMO).unwrap();
    assert!(leftovers.iter().all(|path| !path.exists()), "{leftovers:?}");
    assert!(fresh.exists());
}

#[test]
fn a_bundled_skill_lists_its_supporting_files_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    let root = materialize(dir.path(), DEMO).unwrap();
    let catalog = shared(Vec::new(), &root, &["kg-demo"]).unwrap();
    let paths = catalog.skills["kg-demo"].paths();
    assert_eq!(paths.len(), 2, "{paths:?}");
    let long = paths
        .iter()
        .find(|path| path.ends_with("references/long.md"))
        .expect("the reference is listed");
    assert_eq!(std::fs::read_to_string(long).unwrap(), "Long text.\n");
}

#[test]
fn a_native_copy_of_a_bundled_skill_keeps_its_supporting_files() {
    let dir = tempfile::tempdir().unwrap();
    let root = materialize(dir.path(), DEMO).unwrap();
    let catalog = shared(Vec::new(), &root, &["kg-demo"]).unwrap();
    let out = dir.path().join("out");
    write(&out, &catalog).unwrap();
    assert_eq!(
        std::fs::read_to_string(out.join("kg-demo/references/long.md")).unwrap(),
        "Long text.\n"
    );
}

#[test]
fn a_plugin_skill_cannot_replace_a_bundled_one() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kg-cli");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("SKILL.md"),
        "---\nname: kg-cli\ndescription: Fake\n---\nFake.",
    )
    .unwrap();
    let mut catalog = Catalog {
        skills: BTreeMap::new(),
        errors: Vec::new(),
    };
    let (skill, body) = Skill::parse(
        "kg-cli",
        "---\nname: kg-cli\ndescription: Real\n---\nReal.",
        SkillSource::Bundled,
        None,
    )
    .unwrap();
    catalog.skills.insert(
        "kg-cli".into(),
        Loaded {
            skill,
            body,
            files: Vec::new(),
        },
    );
    catalog.tier(vec![path], SkillSource::Plugin);
    assert_eq!(catalog.skills["kg-cli"].skill.source, SkillSource::Bundled);
    assert!(
        catalog
            .errors
            .iter()
            .any(|error| error.contains("cannot replace"))
    );
}

#[test]
fn a_user_or_project_skill_cannot_use_the_kg_prefix() {
    for source in [SkillSource::User, SkillSource::Project] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kg-custom");
        std::fs::create_dir_all(&path).unwrap();
        std::fs::write(
            path.join("SKILL.md"),
            "---\nname: kg-custom\ndescription: Fake\n---\nFake.",
        )
        .unwrap();
        let mut catalog = Catalog {
            skills: BTreeMap::new(),
            errors: Vec::new(),
        };
        catalog.tier(vec![path], source);
        assert!(!catalog.skills.contains_key("kg-custom"));
        assert!(
            catalog
                .errors
                .iter()
                .any(|error| error.contains("reserved kg- prefix")),
            "{:?}",
            catalog.errors
        );
    }
}

fn loaded(name: &str, path: Option<&str>, files: Vec<PathBuf>) -> Loaded {
    let text = format!("---\nname: {name}\ndescription: {name} does the thing\n---\nBody.");
    let (skill, body) =
        Skill::parse(name, &text, SkillSource::Bundled, path.map(PathBuf::from)).unwrap();
    Loaded { skill, body, files }
}

#[test]
fn valid_name_is_lowercase_words_separated_by_single_dashes() {
    for name in ["a", "a1", "kg-cli", "1-2-3"] {
        assert!(valid_name(name), "{name}");
    }
    for name in ["", "-", "a-", "-a", "a--b", "A", "a_b", "a.b", "a b", "ä"] {
        assert!(!valid_name(name), "{name}");
    }
}

#[test]
fn a_catalog_prompt_lists_each_skill_and_error() {
    let mut catalog = Catalog {
        skills: BTreeMap::new(),
        errors: vec!["plugin dev.test: bad manifest".into()],
    };
    catalog
        .skills
        .insert("kg-cli".into(), loaded("kg-cli", None, Vec::new()));
    let prompt = catalog.prompt();
    assert!(prompt.contains("kg-cli: kg-cli does the thing"), "{prompt}");
    assert!(
        prompt.contains("Skill error: plugin dev.test: bad manifest"),
        "{prompt}"
    );
}

#[test]
fn a_loaded_skill_resolves_its_files_beside_skill_md() {
    let loaded = loaded(
        "kg-cli",
        Some("/base/kg-cli/SKILL.md"),
        vec![PathBuf::from("reference.md")],
    );
    assert_eq!(
        loaded.paths(),
        vec![PathBuf::from("/base/kg-cli/reference.md")]
    );
}

fn command(usage: Option<&str>) -> kg_domain::PluginCommand {
    kg_domain::PluginCommand {
        plugin: kg_domain::PluginId("dev.test.probe".into()),
        group: "probe".into(),
        command: "hang".into(),
        usage: usage.map(Into::into),
    }
}

#[test]
fn no_plugin_commands_means_no_generated_skill() {
    assert_eq!(super::plugin_commands::skill(&[]), None);
}

#[test]
fn the_generated_skill_lists_each_command_with_its_usage_and_plugin() {
    let text =
        super::plugin_commands::skill(&[command(Some("hang <seconds>")), command(None)]).unwrap();
    assert!(
        text.contains("- `kg probe hang <seconds>`, from dev.test.probe"),
        "{text}"
    );
    assert!(
        text.contains("- `kg probe hang`, from dev.test.probe"),
        "{text}"
    );
    let (skill, _) = Skill::parse("kg-plugin-commands", &text, SkillSource::Bundled, None).unwrap();
    assert_eq!(skill.name, "kg-plugin-commands");
}

#[test]
fn a_split_bundled_skill_has_a_short_entry_and_names_only_files_it_ships() {
    for &(name, files) in super::BUNDLED {
        let skill = files
            .iter()
            .find(|(path, _)| *path == "SKILL.md")
            .unwrap()
            .1;
        if matches!(name, "kg-plugin-authoring" | "kg-plugin-port") {
            assert!(
                skill.len() < 4096,
                "{name} SKILL.md is {} bytes",
                skill.len()
            );
        }
        for word in skill.split(|c: char| c.is_whitespace() || "`(),;:\"".contains(c)) {
            let word = word.trim_end_matches('.');
            if word.starts_with("references/") {
                assert!(
                    files.iter().any(|(path, _)| *path == word),
                    "{name} names {word}, which it does not ship"
                );
            }
        }
    }
}

#[test]
fn a_plugin_skill_cannot_take_the_generated_name() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kg-plugin-commands");
    std::fs::create_dir_all(&path).unwrap();
    std::fs::write(
        path.join("SKILL.md"),
        "---\nname: kg-plugin-commands\ndescription: Fake\n---\nFake.",
    )
    .unwrap();
    let mut catalog = Catalog {
        skills: BTreeMap::new(),
        errors: Vec::new(),
    };
    catalog.tier(vec![path], SkillSource::Plugin);
    assert!(!catalog.skills.contains_key("kg-plugin-commands"));
}

#[test]
fn a_loaded_skill_without_a_path_has_no_files() {
    let loaded = loaded("kg-cli", None, vec![PathBuf::from("reference.md")]);
    assert!(loaded.paths().is_empty());
}
