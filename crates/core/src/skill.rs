use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures::lock::Mutex;
use gpui_kit::{AppContext as _, AsyncApp, Entity, WeakEntity};
use kg_domain::{OpError, OpResult, ProjectId, Skill, SkillOp, SkillSource};

use crate::Workspace;
use crate::database::Result;
use crate::workspace::{closed, missing};

const BUNDLED: &[(&str, &str)] = &[
    ("kg-cli", include_str!("../skills/kg-cli.md")),
    (
        "kg-plugin-authoring",
        include_str!("../skills/kg-plugin-authoring.md"),
    ),
    (
        "kg-plugin-port",
        include_str!("../skills/kg-plugin-port.md"),
    ),
    (
        "kg-skill-creator",
        include_str!("../skills/kg-skill-creator.md"),
    ),
    ("kg-threads", include_str!("../skills/kg-threads.md")),
    ("kg-ui", include_str!("../skills/kg-ui.md")),
];

struct Loaded {
    skill: Skill,
    body: String,
    files: Vec<PathBuf>,
    text: Option<&'static str>,
}

impl Loaded {
    fn paths(&self) -> Vec<PathBuf> {
        let Some(root) = self.skill.path.as_ref().and_then(|path| path.parent()) else {
            return Vec::new();
        };
        self.files.iter().map(|file| root.join(file)).collect()
    }
}

pub(crate) struct Catalog {
    skills: BTreeMap<String, Loaded>,
    errors: Vec<String>,
}

impl Catalog {
    pub(crate) fn prompt(&self) -> String {
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

    fn get(&mut self, name: &str) -> Result<Loaded> {
        self.skills
            .remove(name)
            .ok_or_else(|| OpError::NotFound(format!("skill {name} is unavailable")))
    }
}

pub(crate) async fn catalog(
    this: &WeakEntity<Workspace>,
    project_id: Option<ProjectId>,
    cx: &mut AsyncApp,
) -> Result<Catalog> {
    let (data_dir, project, database) = this
        .update(cx, |workspace, cx| {
            let project = project_id.as_ref().map(|id| {
                workspace
                    .projects
                    .get(id)
                    .map(|entity| entity.read(cx).record.path.clone())
            });
            (
                workspace.data_dir.clone(),
                project,
                workspace.database.clone(),
            )
        })
        .map_err(closed)?;
    let project = match project {
        Some(Some(path)) => Some(path),
        Some(None) => {
            return Err(missing(
                "project",
                project_id
                    .as_ref()
                    .ok_or_else(|| OpError::Invalid("project is missing".into()))?,
            ));
        }
        None => None,
    };
    let plugins = database.call(|store| store.plugins()).await?;
    cx.background_spawn(async move {
        let mut catalog = shared(plugins)?;
        catalog.folder(&data_dir.join("skills"), SkillSource::User);
        if let Some(project) = project {
            catalog.folder(&project.join(".kagan/skills"), SkillSource::Project);
        }
        Ok(catalog)
    })
    .await
}

fn shared(plugins: Vec<kg_domain::Plugin>) -> Result<Catalog> {
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

static MIRROR: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) async fn mirror(
    this: &WeakEntity<Workspace>,
    root: PathBuf,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let _turn = MIRROR.get_or_init(|| Mutex::new(())).lock().await;
    let plugins = database.call(|store| store.plugins()).await?;
    cx.background_spawn(async move {
        let catalog = shared(plugins)?;
        write(&root, &catalog)
    })
    .await
}

// The folder is built beside `root` and then renamed into place, so a reader sees the old folder or
// the new one, never a half-written file. The rename is not atomic across processes.
fn write(root: &Path, catalog: &Catalog) -> Result<()> {
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
    fn folder(&mut self, root: &Path, source: SkillSource) {
        match directories(root) {
            Ok(paths) => self.tier(paths, source),
            Err(error) => self.errors.push(format!("{}: {error}", root.display())),
        }
    }

    fn tier(&mut self, paths: Vec<PathBuf>, source: SkillSource) {
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

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

fn read(root: &Path, source: SkillSource) -> Result<Loaded> {
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

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: SkillOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let project_id = match &op {
        SkillOp::List { project_id }
        | SkillOp::Show { project_id, .. }
        | SkillOp::Files { project_id, .. }
        | SkillOp::New { project_id, .. }
        | SkillOp::Add { project_id, .. }
        | SkillOp::Remove { project_id, .. } => project_id.clone(),
    };
    let mut catalog = catalog(this, project_id.clone(), cx).await?;
    match op {
        SkillOp::List { .. } => Ok(OpResult::Skills {
            skills: catalog
                .skills
                .into_values()
                .map(|loaded| loaded.skill)
                .collect(),
            errors: catalog.errors,
        }),
        SkillOp::Show { name, .. } => {
            let loaded = catalog.get(&name)?;
            let files = loaded.paths();
            Ok(OpResult::Skill {
                skill: loaded.skill,
                body: loaded.body,
                files,
            })
        }
        SkillOp::Files { name, .. } => Ok(OpResult::Files(catalog.get(&name)?.paths())),
        SkillOp::New { name, project, .. } => {
            if !valid_name(&name) {
                return Err(OpError::Invalid(
                    "skill name must be lowercase and hyphenated".into(),
                ));
            }
            let root = destination(this, project, project_id.as_ref(), cx)?.join(&name);
            cx.background_spawn(async move {
                std::fs::create_dir_all(root.parent().ok_or_else(|| OpError::Invalid("skill parent is missing".into()))?)?;
                std::fs::create_dir(&root)?;
                std::fs::write(root.join("SKILL.md"), format!("---\nname: {name}\ndescription: Describe when to use this skill.\n---\n\nWrite the instructions here.\n"))?;
                Ok(OpResult::Files(vec![root.join("SKILL.md")]))
            }).await
        }
        SkillOp::Add { path, project, .. } => {
            let target = destination(this, project, project_id.as_ref(), cx)?;
            cx.background_spawn(async move {
                let loaded = read(&path, SkillSource::User)?;
                let target = target.join(&loaded.skill.name);
                std::fs::create_dir_all(
                    target
                        .parent()
                        .ok_or_else(|| OpError::Invalid("skill parent is missing".into()))?,
                )?;
                std::fs::create_dir(&target)?;
                for file in loaded.files {
                    let destination = target.join(&file);
                    if let Some(parent) = destination.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(path.join(file), destination)?;
                }
                Ok(OpResult::Done)
            })
            .await
        }
        SkillOp::Remove { name, .. } => {
            let loaded = catalog.get(&name)?;
            if !matches!(
                loaded.skill.source,
                SkillSource::User | SkillSource::Project
            ) {
                return Err(OpError::Invalid(
                    "only user and project skills can be removed".into(),
                ));
            }
            let path = loaded
                .skill
                .path
                .and_then(|path| path.parent().map(Path::to_owned))
                .ok_or_else(|| OpError::Invalid("skill folder is missing".into()))?;
            cx.background_spawn(async move {
                std::fs::remove_dir_all(path)?;
                Ok(OpResult::Done)
            })
            .await
        }
    }
}

fn destination(
    this: &WeakEntity<Workspace>,
    project: bool,
    id: Option<&ProjectId>,
    cx: &mut AsyncApp,
) -> Result<PathBuf> {
    this.update(cx, |workspace, cx| {
        if project {
            let id = id.ok_or_else(|| {
                OpError::Invalid("--project needs KG_PROJECT_ID or --project-id".into())
            })?;
            let project: &Entity<crate::Project> = workspace
                .projects
                .get(id)
                .ok_or_else(|| missing("project", id))?;
            Ok(project.read(cx).record.path.join(".kagan/skills"))
        } else {
            Ok(workspace.data_dir.join("skills"))
        }
    })
    .map_err(closed)?
}

pub(crate) async fn tools(
    this: &WeakEntity<Workspace>,
    thread: &Entity<crate::Thread>,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let (id, project_id) = thread.read_with(cx, |thread, _| {
        (thread.record.id.clone(), thread.record.project_id.clone())
    });
    let catalog = catalog(this, Some(project_id), cx).await?;
    let instruct = thread.read_with(cx, |thread, _| thread.instruct.clone());
    let note = match instruct.as_deref() {
        Some(extra) if !extra.is_empty() => crate::thread::note_with(&id.0, extra),
        _ => crate::thread::note(&id.0),
    };
    let result =
        crate::thread::forward(this, crate::PluginRequest::Tools { thread_id: id }, cx).await?;
    let OpResult::Tools {
        instructions,
        mut tools,
    } = result
    else {
        return Err(OpError::Failed("plugin host returned no tools".into()));
    };
    tools.retain(|tool| tool.name != "skill_load");
    tools.push(kg_domain::AgentTool { name: "skill_load".into(), description: "Load a skill's instructions and file list".into(), input_schema: serde_json::json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"], "additionalProperties": false }) });
    Ok(OpResult::Tools {
        instructions: Some([instructions.unwrap_or_default(), note, catalog.prompt()].join("\n\n")),
        tools,
    })
}

pub(crate) async fn load(
    this: &WeakEntity<Workspace>,
    thread: &Entity<crate::Thread>,
    input: &serde_json::Value,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let name = input
        .get("name")
        .and_then(serde_json::Value::as_str)
        .filter(|_| input.as_object().is_some_and(|fields| fields.len() == 1))
        .ok_or_else(|| OpError::Invalid("skill_load needs only a string name".into()))?;
    let project_id = thread.read_with(cx, |thread, _| thread.record.project_id.clone());
    let result = run(
        this,
        SkillOp::Show {
            name: name.into(),
            project_id: Some(project_id),
        },
        cx,
    )
    .await?;
    Ok(OpResult::ToolReply {
        text: serde_json::to_string(&result)?,
        is_error: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
