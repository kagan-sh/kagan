use kg_domain::{OpError, Skill, SkillSource};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::database::Result;
use crate::workspace::closed;

use super::BUNDLED;

pub(super) struct Loaded {
    pub(super) skill: Skill,
    pub(super) body: String,
    pub(super) files: Vec<PathBuf>,
    pub(super) text: Option<&'static str>,
}

impl Loaded {
    pub(super) fn paths(&self) -> Vec<PathBuf> {
        let Some(root) = self.skill.path.as_ref().and_then(|path| path.parent()) else {
            return Vec::new();
        };
        self.files.iter().map(|file| root.join(file)).collect()
    }
}

pub(crate) struct Catalog {
    pub(super) skills: BTreeMap<String, Loaded>,
    pub(super) errors: Vec<String>,
}

impl Catalog {
    pub(super) fn prompt(&self) -> String {
        let mut lines = vec![
            "Available skills (load with skill_load { name }, or kg skill show <name>):".into(),
        ];
        lines.extend(
            self.skills
                .values()
                .map(|loaded| format!("- {}: {}", loaded.skill.name, loaded.skill.description)),
        );
        lines.extend(
            self.errors
                .iter()
                .map(|error| format!("Skill error: {error}")),
        );
        lines.join("\n")
    }

    pub(super) fn get(&mut self, name: &str) -> Result<Loaded> {
        self.skills
            .remove(name)
            .ok_or_else(|| OpError::NotFound(format!("skill {name} is unavailable")))
    }
}
pub(super) fn shared(plugins: Vec<kg_domain::Plugin>) -> Result<Catalog> {
    let mut catalog = Catalog {
        skills: BTreeMap::new(),
        errors: Vec::new(),
    };
    for &(name, text) in BUNDLED {
        let (skill, body) = Skill::parse(name, text, SkillSource::Bundled, None)?;
        catalog.skills.insert(
            name.into(),
            Loaded {
                skill,
                body,
                files: Vec::new(),
                text: Some(text),
            },
        );
    }
    let mut paths = Vec::new();
    for plugin in plugins.into_iter().filter(|plugin| plugin.enabled) {
        let result = (|| -> Result<()> {
            let root = dunce::canonicalize(&plugin.root)?;
            let manifest: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(root.join("kagan.json"))?)?;
            if let Some(skills) = manifest.get("skills") {
                let skills: Vec<PathBuf> = serde_json::from_value(skills.clone())?;
                for path in skills {
                    let path = dunce::canonicalize(root.join(path))?;
                    if !path.starts_with(&root) {
                        return Err(OpError::Invalid(format!(
                            "plugin {} skill escapes its folder",
                            plugin.id
                        )));
                    }
                    paths.push(path);
                }
            }
            Ok(())
        })();
        if let Err(error) = result {
            catalog
                .errors
                .push(format!("plugin {}: {error}", plugin.id));
        }
    }
    catalog.tier(paths, SkillSource::Plugin);
    Ok(catalog)
}
// The folder is built beside `root` and then renamed into place, so a reader sees the old folder or
// the new one, never a half-written file. The rename is not atomic across processes.
pub(super) fn write(root: &Path, catalog: &Catalog) -> Result<()> {
    let next = root.with_file_name("skills.next");
    let previous = root.with_file_name("skills.previous");
    clear(&next)?;
    clear(&previous)?;
    std::fs::create_dir_all(&next)?;
    for (name, loaded) in &catalog.skills {
        let folder = next.join(name);
        std::fs::create_dir_all(&folder)?;
        if let Some(text) = loaded.text {
            std::fs::write(folder.join("SKILL.md"), text)?;
            continue;
        }
        let source = loaded
            .skill
            .path
            .as_deref()
            .and_then(Path::parent)
            .ok_or_else(|| OpError::Invalid(format!("skill {name} has no folder")))?;
        for file in &loaded.files {
            let target = folder.join(file);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(source.join(file), target)?;
        }
    }
    if root.exists() {
        std::fs::rename(root, &previous)?;
    }
    std::fs::rename(&next, root)?;
    clear(&previous)?;
    Ok(())
}

fn clear(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

impl Catalog {
    pub(super) fn folder(&mut self, root: &Path, source: SkillSource) {
        match directories(root) {
            Ok(paths) => self.tier(paths, source),
            Err(error) => self.errors.push(format!("{}: {error}", root.display())),
        }
    }

    pub(super) fn tier(&mut self, paths: Vec<PathBuf>, source: SkillSource) {
        let mut tier = BTreeMap::new();
        let mut duplicates = BTreeSet::new();
        for path in paths {
            match read(&path, source) {
                Ok(loaded) => {
                    let name = loaded.skill.name.clone();
                    if tier.insert(name.clone(), loaded).is_some() {
                        duplicates.insert(name);
                    }
                }
                Err(error) => self.errors.push(format!("{}: {error}", path.display())),
            }
        }
        for name in duplicates {
            tier.remove(&name);
            self.errors
                .push(format!("duplicate {source:?} skill {name}; both dropped"));
        }
        for (name, loaded) in tier {
            if source == SkillSource::Plugin
                && self
                    .skills
                    .get(&name)
                    .is_some_and(|kept| kept.skill.source == SkillSource::Bundled)
            {
                self.errors.push(format!(
                    "plugin skill {name} cannot replace the bundled skill"
                ));
                continue;
            }
            self.skills.insert(name, loaded);
        }
    }
}

fn directories(root: &Path) -> Result<Vec<PathBuf>> {
    let entries = match std::fs::read_dir(root) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    Ok(paths)
}

pub(super) fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

pub(super) fn read(root: &Path, source: SkillSource) -> Result<Loaded> {
    let root = dunce::canonicalize(root)?;
    let root = root.as_path();
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| OpError::Invalid("skill folder needs a UTF-8 name".into()))?;
    let mut files = Vec::new();
    files_in(root, root, &mut files)?;
    let text = std::fs::read_to_string(root.join("SKILL.md"))?;
    let (skill, body) = Skill::parse(name, &text, source, Some(root.join("SKILL.md")))?;
    files.sort();
    Ok(Loaded {
        skill,
        body,
        files,
        text: None,
    })
}

fn files_in(root: &Path, folder: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(OpError::Invalid(format!(
                "skill contains a link: {}",
                entry.path().display()
            )));
        }
        if kind.is_dir() {
            files_in(root, &entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path().strip_prefix(root).map_err(closed)?.to_owned());
        }
    }
    Ok(())
}
