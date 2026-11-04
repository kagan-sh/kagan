use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    EnvironmentId, PendingInteractionId, PluginId, ProjectId, SectionId, ThreadId, Timestamp,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProjectKind {
    Standard,
    Personal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentStatus {
    Creating,
    Provisioning,
    Ready,
    Error,
    Destroyed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EnvironmentProvider {
    Project,
    Worktree,
    Personal,
}

impl EnvironmentProvider {
    /// Kagan created the folder. A worktree is removed after its last thread is archived; a personal folder stays until Delete removes it.
    #[must_use]
    pub fn managed(self) -> bool {
        matches!(self, Self::Worktree | Self::Personal)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThreadStatus {
    Pending,
    Idle,
    Starting,
    Active,
    Stopping,
    Error,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum Visibility {
    #[default]
    Visible,
    Hidden,
}

/// A thread whose title or kept terminal lines hold the searched text.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Found {
    pub thread_id: ThreadId,
    pub title: String,
    pub project_id: ProjectId,
    pub project: String,
    pub archived: bool,
    /// The last kept line that holds the text; none when only the title does.
    pub snippet: Option<Snippet>,
}

/// One kept terminal line around the match, and how many lines to scroll back to show it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snippet {
    pub text: String,
    pub scroll: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: ProjectId,
    pub kind: ProjectKind,
    pub name: String,
    pub path: PathBuf,
    pub git_remote_url: Option<String>,
    /// Rank among standard projects; Personal stays last regardless.
    #[serde(default)]
    pub sort: i64,
}

pub fn order_sidebar_projects(projects: &mut [Project]) {
    projects.sort_by(|left, right| {
        let personal = |project: &Project| u8::from(matches!(project.kind, ProjectKind::Personal));
        personal(left)
            .cmp(&personal(right))
            .then_with(|| left.sort.cmp(&right.sort))
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.id.0.cmp(&right.id.0))
    });
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Environment {
    pub id: EnvironmentId,
    pub project_id: ProjectId,
    pub path: PathBuf,
    pub status: EnvironmentStatus,
    pub provider: EnvironmentProvider,
    pub branch: Option<String>,
    pub base_branch: Option<String>,
    pub retire_at: Option<Timestamp>,
    /// The reason a removal is refused. Nothing else sets it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThreadMark {
    #[default]
    None,
    Done,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Thread {
    pub id: ThreadId,
    pub project_id: ProjectId,
    pub environment_id: Option<EnvironmentId>,
    pub title: String,
    /// A socket rename leaves a locked title unchanged and fails. Any other rename still replaces it.
    #[serde(default)]
    pub title_locked: bool,
    pub provider: String,
    pub model: String,
    pub reasoning_level: String,
    pub status: ThreadStatus,
    pub visibility: Visibility,
    pub parent_thread_id: Option<ThreadId>,
    pub lifecycle_owner_thread_id: Option<ThreadId>,
    pub section_id: Option<SectionId>,
    pub pinned: bool,
    pub last_read_at: Option<Timestamp>,
    pub latest_attention_at: Timestamp,
    #[serde(default)]
    pub mark: ThreadMark,
    pub archived_at: Option<Timestamp>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl Thread {
    #[must_use]
    pub fn unread(&self) -> bool {
        self.last_read_at
            .is_none_or(|read| self.latest_attention_at > read)
    }
}

/// `id` first, then every thread whose lifecycle owner is already in the list.
#[must_use]
pub fn owned_thread_ids(threads: &[Thread], id: &ThreadId) -> Vec<ThreadId> {
    let mut placed = BTreeSet::from([id.clone()]);
    let mut ids = vec![id.clone()];
    loop {
        let mut progressed = false;
        for thread in threads {
            if placed.contains(&thread.id) {
                continue;
            }
            if thread
                .lifecycle_owner_thread_id
                .as_ref()
                .is_some_and(|owner| placed.contains(owner))
            {
                placed.insert(thread.id.clone());
                ids.push(thread.id.clone());
                progressed = true;
            }
        }
        if !progressed {
            break;
        }
    }
    ids
}

/// Visible, unarchived threads archived with `id` because it owns their lifetime.
/// A hidden or archived owner is not counted, and still links its visible descendants.
#[must_use]
pub fn archive_span(threads: &[Thread], id: &ThreadId) -> usize {
    let owned = owned_thread_ids(threads, id);
    threads
        .iter()
        .filter(|thread| {
            thread.id != *id
                && owned.contains(&thread.id)
                && thread.archived_at.is_none()
                && thread.visibility == Visibility::Visible
        })
        .count()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Section {
    pub id: SectionId,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PendingInteraction {
    pub id: PendingInteractionId,
    pub thread_id: ThreadId,
    pub title: String,
    pub options: Vec<InteractionOption>,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InteractionOption {
    pub id: String,
    pub label: String,
}

impl InteractionOption {
    /// The answer that lets a permission question proceed.
    pub const APPROVE: &str = "approve";
    /// The answer that cancels a permission question.
    pub const DECLINE: &str = "decline";
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sidebar {
    pub projects: Vec<Project>,
    pub sections: Vec<Section>,
    pub threads: Vec<Thread>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentTool {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// One plugin in an index file, as `kg plugin share` prints it. `source` is what
/// `kg plugin install` takes, and `plugin` its `--plugin`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IndexEntry {
    pub name: String,
    pub description: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// A plugin an index lists: `index` is the name the index gives itself, and `location` its
/// address without a password.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Listing {
    pub index: String,
    pub location: String,
    pub name: String,
    pub description: String,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plugin: Option<String>,
}

/// `approved` holds the powers, the folder settings, and the folder chosen for each;
/// the plugins crate owns its shape. `commit` is set for git installs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Plugin {
    pub id: PluginId,
    pub root: PathBuf,
    pub source: String,
    pub commit: Option<String>,
    pub enabled: bool,
    pub approved: Value,
    /// Why the plugin did not start. Filled when plugins are listed; never stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
