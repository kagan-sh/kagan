use std::path::{Path, PathBuf};

use kg_domain::PluginId;
use serde_json::Value;

use crate::contributions::{Capability, Contributions};
use crate::manifest::{self, Area};

/// A plugin shipped in the repository for Browse and Create.
#[derive(Clone, Debug)]
pub struct Example {
    pub id: PluginId,
    pub name: String,
    pub description: String,
    pub kinds: Vec<String>,
    pub kind_ids: Vec<String>,
    pub icon: Vec<u8>,
    pub root: PathBuf,
}

/// Contribution kinds from [`Contributions`] and the other manifest keys, then each host area.
#[must_use]
pub fn capabilities() -> Vec<Capability> {
    let mut kinds = Contributions::kinds().to_vec();
    kinds.extend(DECLARED);
    kinds.extend(Area::ALL.into_iter().map(|area| Capability {
        id: area.id(),
        name: area.name(),
        detail: area.phrase(),
    }));
    kinds
}

/// Manifest keys besides `ui` that declare a contribution. Each id is a key in the manifest.
const DECLARED: [Capability; 6] = [
    Capability {
        id: "commands",
        name: "Command",
        detail: "adds a command",
    },
    Capability {
        id: "tools",
        name: "Agent tool",
        detail: "adds an agent tool",
    },
    Capability {
        id: "cli",
        name: "CLI command",
        detail: "adds a CLI command",
    },
    Capability {
        id: "skills",
        name: "Skill",
        detail: "ships a skill",
    },
    Capability {
        id: "setting",
        name: "Setting",
        detail: "declares a setting",
    },
    Capability {
        id: "themes",
        name: "Theme",
        detail: "ships a theme",
    },
];

/// Capabilities present on a `kagan.json` value.
#[must_use]
pub fn matched(value: &Value) -> Vec<Capability> {
    capabilities()
        .into_iter()
        .filter(|kind| present(value, kind.id))
        .collect()
}

/// Kind names present on a `kagan.json` value.
#[must_use]
pub fn kinds_in(value: &Value) -> Vec<String> {
    matched(value)
        .into_iter()
        .map(|kind| kind.name.to_owned())
        .collect()
}

fn present(value: &Value, id: &str) -> bool {
    let filled = |value: &Value| match value {
        Value::Array(items) => !items.is_empty(),
        Value::Object(map) => !map.is_empty(),
        Value::String(text) => !text.is_empty(),
        Value::Null | Value::Bool(false) => false,
        Value::Bool(true) | Value::Number(_) => true,
    };
    if id == "setting" {
        return value.get("settings").is_some_and(filled);
    }
    if Area::ALL.iter().any(|area| area.id() == id) {
        return value["powers"]["areas"]
            .as_array()
            .is_some_and(|areas| areas.iter().any(|area| area.as_str() == Some(id)));
    }
    if Contributions::kinds().iter().any(|kind| kind.id == id) {
        return value["ui"].get(id).is_some_and(filled);
    }
    value.get(id).is_some_and(filled)
}

/// Example plugins under `examples/plugins`, in name order.
#[must_use]
pub fn examples() -> Vec<Example> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/plugins");
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Some(example) = read_example(&path)
        {
            found.push(example);
        }
    }
    found.sort_by(|left, right| left.name.cmp(&right.name));
    found
}

fn read_example(root: &Path) -> Option<Example> {
    let text = std::fs::read_to_string(root.join(manifest::FILE)).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    let id = PluginId(value["id"].as_str()?.to_owned());
    let name = value["name"].as_str().unwrap_or(id.0.as_str()).to_owned();
    let description = value["instructions"]
        .as_str()
        .filter(|text| !text.is_empty())
        .unwrap_or("Bundled example")
        .to_owned();
    let icon = value["icon"]
        .as_str()
        .map(|icon| root.join(icon))
        .and_then(|path| std::fs::read(path).ok())
        .unwrap_or_default();
    let matched = matched(&value);
    Some(Example {
        id,
        name,
        description,
        kinds: matched.iter().map(|kind| kind.name.to_owned()).collect(),
        kind_ids: matched.iter().map(|kind| kind.id.to_owned()).collect(),
        icon,
        root: root.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn bundled_examples_pass_check() {
        let examples = super::examples();
        assert_eq!(examples.len(), 7);
        for example in &examples {
            let data = tempfile::tempdir().unwrap();
            crate::development::check(&example.root, data.path()).unwrap();
            assert!(!example.kinds.is_empty(), "{}", example.name);
        }
        let ids: Vec<_> = super::capabilities()
            .into_iter()
            .map(|kind| kind.id)
            .collect();
        assert!(ids.contains(&"panels"));
        assert!(ids.contains(&"cli"));
        assert!(ids.contains(&"threads"));
    }
}
