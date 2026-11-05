use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(transparent)]
pub(crate) struct Settings(IndexMap<String, Setting>);

impl Settings {
    pub(crate) fn get(&self, key: &str) -> Option<&Setting> {
        self.0.get(key)
    }

    pub(crate) fn contains_key(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&String, &Setting)> {
        self.0.iter()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &Setting> {
        self.0.values()
    }

    pub(crate) fn secrets(&self) -> Vec<String> {
        let secret = |(key, setting): (&String, &Setting)| {
            matches!(setting, Setting::Secret { .. }).then(|| key.clone())
        };
        self.0.iter().filter_map(secret).collect()
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase", deny_unknown_fields)]
pub enum Setting {
    String {
        title: String,
        #[serde(default)]
        default: Option<String>,
    },
    Boolean {
        title: String,
        #[serde(default)]
        default: Option<bool>,
    },
    Number {
        title: String,
        #[serde(default)]
        default: Option<serde_json::Number>,
    },
    Select {
        title: String,
        options: Vec<String>,
        #[serde(default)]
        default: Option<String>,
    },
    Folder {
        title: String,
    },
    Project {
        title: String,
    },
    Provider {
        title: String,
    },
    Secret {
        title: String,
    },
}

/// The person's pick for a `provider` setting. Only kagan reads it; the plugin passes the
/// setting's key to `spawn` and never sees the value.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AgentChoice {
    pub provider: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_level: Option<String>,
    #[serde(default)]
    pub permission_mode: Option<String>,
}

impl Setting {
    #[must_use]
    pub fn title(&self) -> &str {
        match self {
            Self::String { title, .. }
            | Self::Boolean { title, .. }
            | Self::Number { title, .. }
            | Self::Select { title, .. }
            | Self::Folder { title }
            | Self::Project { title }
            | Self::Provider { title }
            | Self::Secret { title } => title,
        }
    }

    pub(crate) fn default_value(&self) -> Value {
        match self {
            Self::String { default, .. } | Self::Select { default, .. } => default.clone().into(),
            Self::Boolean { default, .. } => (*default).into(),
            Self::Number { default, .. } => default.clone().map_or(Value::Null, Value::Number),
            Self::Folder { .. }
            | Self::Project { .. }
            | Self::Provider { .. }
            | Self::Secret { .. } => Value::Null,
        }
    }
}
