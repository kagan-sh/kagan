use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures::lock::Mutex;
use gpui_kit::{AppContext as _, AsyncApp, Entity, WeakEntity};
use kg_domain::{OpError, OpResult, ProjectId, SkillOp, SkillSource};

use crate::Workspace;
use crate::host::database::Result;
use crate::workspace::{closed, missing};

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

use catalog::{Catalog, read, shared, valid_name, write};

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
static MIRROR: OnceLock<Mutex<()>> = OnceLock::new();

pub(crate) async fn mirror(
    this: &WeakEntity<Workspace>,
    root: PathBuf,
    cx: &mut AsyncApp,
) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let plugins = database.call(|store| store.plugins()).await?;
    cx.background_spawn(async move {
        let catalog = shared(plugins)?;
        let _turn = MIRROR.get_or_init(|| Mutex::new(())).lock().await;
        write(&root, &catalog)
    })
    .await
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
        crate::dispatch::forward(this, crate::PluginRequest::Tools { thread_id: id }, cx).await?;
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
mod tests;
