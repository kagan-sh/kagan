use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use super::fetch::{Clipboard, Env, Fetch, FetchKind, Method, Powers};
use super::{Manifest, Setting};

/// The approved form of [`Powers`], and the whole of what a plugin may do.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Grant {
    #[serde(default)]
    pub(crate) powers: Powers,
    #[serde(default)]
    pub(crate) folders: BTreeMap<String, String>,
    /// The folder a person approved for each folder setting, canonical.
    #[serde(default)]
    pub(crate) paths: BTreeMap<String, std::path::PathBuf>,
    #[serde(default)]
    pub(crate) page_store: bool,
    #[serde(default)]
    pub(crate) instruct: bool,
    #[serde(default)]
    pub(crate) skills: bool,
}

impl Grant {
    pub(crate) fn of(manifest: &Manifest, root: &Path) -> Self {
        Self {
            powers: manifest.powers.clone(),
            folders: manifest
                .settings
                .iter()
                .filter_map(|(key, setting)| match setting {
                    Setting::Folder { title } => Some((key.clone(), title.clone())),
                    Setting::String { .. }
                    | Setting::Boolean { .. }
                    | Setting::Number { .. }
                    | Setting::Select { .. }
                    | Setting::Project { .. }
                    | Setting::Provider { .. }
                    | Setting::Secret { .. } => None,
                })
                .collect(),
            paths: BTreeMap::new(),
            page_store: manifest.ui.panels.iter().any(|panel| panel.store)
                || crate::development::calls_webview_store(root),
            instruct: manifest.instruct,
            skills: !manifest.skills.is_empty(),
        }
    }

    pub(crate) fn keep_paths(&mut self, approved: &Self) {
        self.paths = approved
            .paths
            .iter()
            .filter(|(key, _)| self.folders.contains_key(*key))
            .map(|(key, path)| (key.clone(), path.clone()))
            .collect();
    }

    /// What both grants allow, keeping `self`'s approved folder paths.
    pub(crate) fn intersect(&self, other: &Self) -> Self {
        let (mine, theirs) = (&self.powers, &other.powers);
        let folders: BTreeMap<String, String> = self
            .folders
            .iter()
            .filter(|(key, _)| other.folders.contains_key(*key))
            .map(|(key, title)| (key.clone(), title.clone()))
            .collect();
        Self {
            powers: Powers {
                run: theirs
                    .run
                    .iter()
                    .filter(|program| mine.run.iter().any(|approved| program.within(approved)))
                    .cloned()
                    .collect(),
                fetch: mine
                    .fetch
                    .iter()
                    .filter_map(|fetch| {
                        let other = theirs.fetch.iter().find(|other| other.host == fetch.host)?;
                        let methods: BTreeSet<Method> = fetch
                            .methods
                            .intersection(&other.methods)
                            .copied()
                            .collect();
                        (!methods.is_empty()).then(|| Fetch {
                            host: fetch.host.clone(),
                            methods,
                        })
                    })
                    .collect(),
                clipboard: Clipboard {
                    read: mine.clipboard.read && theirs.clipboard.read,
                    write: mine.clipboard.write && theirs.clipboard.write,
                },
                areas: mine.areas.intersection(&theirs.areas).copied().collect(),
                read: theirs
                    .read
                    .iter()
                    .filter(|entry| mine.read.iter().any(|approved| approved == *entry))
                    .cloned()
                    .collect(),
                open_url: mine.open_url && theirs.open_url,
            },
            paths: self
                .paths
                .iter()
                .filter(|(key, _)| folders.contains_key(*key))
                .map(|(key, path)| (key.clone(), path.clone()))
                .collect(),
            folders,
            page_store: self.page_store && other.page_store,
            instruct: self.instruct && other.instruct,
            skills: self.skills && other.skills,
        }
    }

    /// Whether everything this asks for was already approved in `approved`.
    pub(crate) fn within(&self, approved: &Self) -> bool {
        let (new, old) = (&self.powers, &approved.powers);
        new.run
            .iter()
            .all(|program| old.run.iter().any(|approved| program.within(approved)))
            && new.fetch.iter().all(|fetch| {
                old.fetch
                    .iter()
                    .any(|had| had.host == fetch.host && fetch.methods.is_subset(&had.methods))
            })
            && (!new.clipboard.read || old.clipboard.read)
            && (!new.clipboard.write || old.clipboard.write)
            && (!new.open_url || old.open_url)
            && new.areas.is_subset(&old.areas)
            && new
                .read
                .iter()
                .all(|entry| old.read.iter().any(|approved| approved == entry))
            && self
                .folders
                .keys()
                .all(|key| approved.folders.contains_key(key))
            && (!self.page_store || approved.page_store)
            && (!self.instruct || approved.instruct)
            && (!self.skills || approved.skills)
    }

    /// The lines `approved` did not have; all lines when only details changed.
    pub(crate) fn added(&self, approved: &Self) -> Vec<String> {
        let old = approved.lines();
        let lines = self.lines();
        let new: Vec<String> = lines
            .iter()
            .filter(|line| !old.contains(line))
            .cloned()
            .collect();
        if new.is_empty() { lines } else { new }
    }

    /// The install screen, one plain line per power.
    pub(crate) fn lines(&self) -> Vec<String> {
        let powers = &self.powers;
        let mut lines = Vec::new();
        for program in &powers.run {
            lines.push(match program.env {
                Some(Env::Inherit) => format!("runs {} with your environment", program.program),
                None => format!("runs {}", program.program),
            });
        }
        let mut local_ports = BTreeSet::new();
        for fetch in &powers.fetch {
            let methods = fetch
                .methods
                .iter()
                .map(|method| method.name())
                .collect::<Vec<_>>()
                .join(", ");
            match fetch.kind() {
                Ok(FetchKind::Loopback { port, .. }) => {
                    if local_ports.insert(port) {
                        lines.push(format!("talks to programs on this computer (port {port})"));
                    }
                }
                _ => lines.push(format!("talks to {} ({methods})", fetch.host)),
            }
        }
        if powers.clipboard.read {
            lines.push("reads your clipboard".into());
        }
        if powers.clipboard.write {
            lines.push("writes to your clipboard".into());
        }
        for folder in &powers.read {
            lines.push(format!("reads {folder}"));
        }
        for area in &powers.areas {
            lines.push(area.phrase().into());
        }
        for title in self.folders.values() {
            lines.push(format!("reads a folder you choose ({title})"));
        }
        if self.page_store {
            lines.push("keeps page data on macOS 14 or later".into());
        }
        if self.instruct {
            lines.push("adds instructions to a thread's program".into());
        }
        if self.skills {
            lines.push("adds skills an agent can load".into());
        }
        if powers.open_url {
            lines.push("opens links in your browser".into());
        }
        lines
    }
}
