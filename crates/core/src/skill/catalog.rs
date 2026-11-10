use kg_domain::{OpError, Skill, SkillSource};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use crate::host::database::Result;
use crate::workspace::closed;

pub(super) struct Loaded {
    pub(super) skill: Skill,
    pub(super) body: String,
    pub(super) files: Vec<PathBuf>,
}

impl Loaded {
    pub(super) fn paths(&self) -> Vec<PathBuf> {
        let Some(root) = self.skill.path.as_ref().and_then(|path| path.parent()) else {
            return Vec::new();
        };
        self.files.iter().map(|file| root.join(file)).collect()
    }
}

#[derive(Default)]
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
/// Writes the skill files, as (skill, path inside the skill, text), under
/// `<data>/agent/bundled/<hash>`, named by their content, and returns that folder. The folder is
/// built beside its final name and renamed into place.
pub(super) fn materialize(data_dir: &Path, files: &[(&str, &str, &str)]) -> Result<PathBuf> {
    use sha2::{Digest as _, Sha256};
    let mut hash = Sha256::new();
    for &(skill, path, text) in files {
        hash.update(format!("{skill}\0{path}\0{text}\0").as_bytes());
    }
    let folder = data_dir.join("agent/bundled");
    let stable = folder.join(hex::encode(hash.finalize()));
    if stable.is_dir() && intact(&stable, files) {
        return Ok(stable);
    }
    std::fs::create_dir_all(&folder)?;
    if stable.exists() {
        // A reader may still be inside a damaged folder, so it is moved aside, not deleted.
        let aside = folder.join(format!(".broken-{}", uuid::Uuid::new_v4()));
        match std::fs::rename(&stable, &aside) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let temp = folder.join(format!(
        ".tmp-{}-{}",
        std::process::id(),
        uuid::Uuid::new_v4()
    ));
    let result = (|| -> Result<()> {
        for &(skill, path, text) in files {
            let target = temp.join(skill).join(path);
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(target, text)?;
        }
        place(&temp, &stable)
    })();
    // After a rename the temp folder is gone; after a failure this tidies it.
    let _ = std::fs::remove_dir_all(&temp);
    result?;
    Ok(stable)
}

/// True when the folder holds exactly these files, with this text, and no link.
fn intact(root: &Path, files: &[(&str, &str, &str)]) -> bool {
    let mut found = Vec::new();
    if files_under(root, root, &mut found).is_err() {
        return false;
    }
    let expected: BTreeSet<PathBuf> = files
        .iter()
        .map(|&(skill, path, _)| Path::new(skill).join(path))
        .collect();
    found.into_iter().collect::<BTreeSet<_>>() == expected
        && files.iter().all(|&(skill, path, text)| {
            std::fs::read_to_string(root.join(skill).join(path)).is_ok_and(|got| got == text)
        })
}

fn files_under(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        if kind.is_symlink() {
            return Err(std::io::Error::other("a link in a bundled folder"));
        }
        if kind.is_dir() {
            files_under(root, &entry.path(), out)?;
        } else {
            out.push(
                entry
                    .path()
                    .strip_prefix(root)
                    .map_err(std::io::Error::other)?
                    .to_owned(),
            );
        }
    }
    Ok(())
}

/// Moves a built folder into its final name. Another reader may have placed the same content first.
pub(super) fn place(temp: &Path, stable: &Path) -> Result<()> {
    match std::fs::rename(temp, stable) {
        Ok(()) => Ok(()),
        Err(_) if stable.is_dir() => Ok(()),
        Err(error) => Err(error.into()),
    }
}

pub(super) fn shared(
    plugins: Vec<kg_domain::Plugin>,
    root: &Path,
    names: &[&str],
) -> Result<Catalog> {
    let mut catalog = Catalog::default();
    for &name in names {
        catalog
            .skills
            .insert(name.into(), read(&root.join(name), SkillSource::Bundled)?);
    }
    let mut paths = Vec::new();
    for plugin in plugins.into_iter().filter(|plugin| plugin.enabled) {
        let result = (|| -> Result<()> {
            let root = dunce::canonicalize(&plugin.root)?;
            for skill in &plugin.skills {
                let path = dunce::canonicalize(root.join(skill))?;
                if !path.starts_with(&root) {
                    return Err(OpError::Invalid(format!(
                        "plugin {} skill escapes its folder",
                        plugin.id
                    )));
                }
                paths.push(path);
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
            if matches!(source, SkillSource::User | SkillSource::Project) && super::reserved(&name)
            {
                self.errors.push(format!(
                    "{source:?} skill {name} uses the reserved kg- prefix; dropped"
                ));
                continue;
            }
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
    Ok(Loaded { skill, body, files })
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
