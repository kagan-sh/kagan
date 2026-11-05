use std::collections::BTreeMap;

use kg_domain::{Skill, SkillSource};

use super::catalog::{Catalog, Loaded};

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
            text: None,
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
