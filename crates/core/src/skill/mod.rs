use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures::lock::Mutex;
use gpui_kit::{AppContext as _, AsyncApp, Entity, WeakEntity};
use kg_domain::{OpError, OpResult, ProjectId, Skill, SkillOp, SkillSource};

use crate::Workspace;
use crate::host::database::Result;
use crate::workspace::{WorkspaceEvent, closed, missing};

const BUNDLED: &[(&str, &str)] = &[
    ("kg-cli", include_str!("../../skills/kg-cli.md")),
    (
        "kg-plugin-authoring",
        include_str!("../../skills/kg-plugin-authoring.md"),
    ),
    (
        "kg-plugin-port",
        include_str!("../../skills/kg-plugin-port.md"),
    ),
    (
        "kg-skill-creator",
        include_str!("../../skills/kg-skill-creator.md"),
    ),
    ("kg-threads", include_str!("../../skills/kg-threads.md")),
    ("kg-ui", include_str!("../../skills/kg-ui.md")),
];

mod catalog;

use catalog::{Catalog, Loaded, read, shared, valid_name, write};

pub(crate) async fn catalog(
    this: &WeakEntity<Workspace>,
    project_id: Option<ProjectId>,
    cx: &mut AsyncApp,
) -> Result<Catalog> {
    let (data_dir, project, database) = this
        .update(cx, |workspace, cx| -> Result<_> {
            let project = project_id
                .as_ref()
                .map(|id| {
                    workspace
                        .projects
                        .get(id)
                        .map(|entity| entity.read(cx).record.path.clone())
                        .ok_or_else(|| missing("project", id))
                })
                .transpose()?;
            Ok((
                workspace.data_dir.clone(),
                project,
                workspace.database.clone(),
            ))
        })
        .map_err(closed)??;
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
/// The tool core serves itself, so a thread can read a skill without the plugin host.
pub(crate) const SKILL_LOAD: &str = "skill_load";

/// Skills named `kg-…` are kagan's own; a person's or a project's skill cannot take the name.
pub(crate) fn reserved(name: &str) -> bool {
    name.starts_with("kg-")
}

static MIRROR: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) async fn mirror(
    this: &WeakEntity<Workspace>,
    agent: PathBuf,
    project_id: ProjectId,
    cx: &mut AsyncApp,
) -> Result<(PathBuf, Vec<String>, Vec<String>)> {
    let resolved = catalog(this, Some(project_id), cx).await?;
    cx.background_spawn(async move {
        let names = resolved.skills.keys().cloned().collect();
        let errors = resolved.errors.clone();
        let _turn = MIRROR.get_or_init(|| Mutex::new(())).lock().await;
        let catalogs = agent.join("catalogs");
        std::fs::create_dir_all(&catalogs)?;
        let temp = catalogs.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let result = (|| -> Result<PathBuf> {
            write(&temp.join("skills"), &resolved)?;
            let plugin = temp.join(".claude-plugin");
            std::fs::create_dir_all(&plugin)?;
            std::fs::write(plugin.join("plugin.json"), "{\"name\": \"kagan\"}\n")?;
            let hash = hash_tree(&temp.join("skills"))?;
            let stable = catalogs.join(hash);
            if stable.is_dir() {
                std::fs::remove_dir_all(&temp)?;
            } else {
                std::fs::rename(&temp, &stable)?;
            }
            Ok(stable.join("skills"))
        })();
        if result.is_err() {
            // The failure below is the one to report; the temp folder is only tidied.
            let _ = std::fs::remove_dir_all(&temp);
        }
        let root = result?;
        Ok((root, names, errors))
    })
    .await
}

fn hash_tree(root: &Path) -> std::io::Result<String> {
    use sha2::{Digest as _, Sha256};
    fn visit(dir: &Path, root: &Path, hash: &mut Sha256) -> std::io::Result<()> {
        let mut entries = std::fs::read_dir(dir)?.collect::<std::io::Result<Vec<_>>>()?;
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            let relative = path.strip_prefix(root).unwrap_or(&path);
            if path.is_dir() {
                hash.update(b"dir\0");
                hash.update(relative.to_string_lossy().as_bytes());
                hash.update(b"\0");
                visit(&path, root, hash)?;
            } else {
                hash.update(b"file\0");
                hash.update(relative.to_string_lossy().as_bytes());
                hash.update(b"\0");
                hash.update(std::fs::read(&path)?);
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    visit(root, root, &mut hash)?;
    Ok(hex::encode(hash.finalize()))
}

pub(crate) async fn run(
    this: &WeakEntity<Workspace>,
    op: SkillOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    if let SkillOp::Open = op {
        this.update(cx, |_, cx| cx.emit(WorkspaceEvent::Skills))
            .map_err(closed)?;
        return Ok(OpResult::Done);
    }
    let project_id = match &op {
        SkillOp::Open => None,
        SkillOp::List { project_id }
        | SkillOp::Show { project_id, .. }
        | SkillOp::Files { project_id, .. }
        | SkillOp::New { project_id, .. }
        | SkillOp::Add { project_id, .. }
        | SkillOp::Update { project_id, .. }
        | SkillOp::Remove { project_id, .. } => project_id.clone(),
    };
    let mut catalog = catalog(this, project_id.clone(), cx).await?;
    match op {
        SkillOp::Open => Ok(OpResult::Done),
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
            if !valid_name(&name) || reserved(&name) {
                return Err(OpError::Invalid(
                    "skill name must be lowercase and hyphenated, and must not start with kg-"
                        .into(),
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
                if reserved(&loaded.skill.name) {
                    return Err(OpError::Invalid(
                        "a skill name must not start with kg-".into(),
                    ));
                }
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
        SkillOp::Update { name, text, .. } => {
            let loaded = catalog.get(&name)?;
            update(loaded, &name, text, cx).await
        }
        SkillOp::Remove { name, .. } => remove(catalog.get(&name)?, cx).await,
    }
}

async fn update(loaded: Loaded, name: &str, text: String, cx: &mut AsyncApp) -> Result<OpResult> {
    let source = loaded.skill.source;
    if !matches!(source, SkillSource::User | SkillSource::Project) {
        return Err(OpError::Invalid(
            "only user and project skills can be edited".into(),
        ));
    }
    let path = loaded
        .skill
        .path
        .ok_or_else(|| OpError::Invalid("skill file is missing".into()))?;
    Skill::parse(name, &text, source, None)?;
    cx.background_spawn(async move {
        std::fs::write(path, text)?;
        Ok(OpResult::Done)
    })
    .await
}

async fn remove(loaded: Loaded, cx: &mut AsyncApp) -> Result<OpResult> {
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
    let data_dir = this
        .update(cx, |workspace, _| workspace.data_dir.clone())
        .map_err(closed)?;
    let instructions = cx
        .background_spawn(async move { crate::thread::read_instructions(&data_dir) })
        .await;
    let note = crate::thread::note_with(&id.0, &instructions, instruct.as_deref().unwrap_or(""));
    let result =
        crate::dispatch::forward(this, crate::PluginRequest::Tools { thread_id: id }, cx).await?;
    let OpResult::Tools {
        instructions,
        mut tools,
    } = result
    else {
        return Err(OpError::Failed("plugin host returned no tools".into()));
    };
    tools.retain(|tool| tool.name != SKILL_LOAD);
    tools.push(kg_domain::AgentTool { name: SKILL_LOAD.into(), description: "Load a skill's instructions and file list".into(), input_schema: serde_json::json!({ "type": "object", "properties": { "name": { "type": "string" } }, "required": ["name"], "additionalProperties": false }) });
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
mod tests;
