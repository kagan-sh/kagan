use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use kg_domain::{OpError, PluginId, ThemeFile};
use serde::{Deserialize, Serialize};
use serde_json::Value;

mod fetch;
mod grant;
mod roots;
mod settings;
pub(crate) use settings::schema_failure;

pub(crate) use fetch::{Area, Env, Fetch, FetchError, FetchKind, LOCAL_FETCH_CAP, Powers};
#[cfg(test)]
pub(crate) use fetch::{Method, Program};
pub(crate) use grant::Grant;
pub(crate) use roots::{ReadRootError, inside, read_root};
pub(crate) use settings::Settings;
pub use settings::{AgentChoice, Setting};

pub(crate) const FILE: &str = "kagan.json";

/// Why `kagan.json` is refused or a grant cannot be used.
#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("cannot read it: {source}")]
    Read { source: std::io::Error },
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("must be a JSON object")]
    NotObject,
    #[error("id {0:?} must be reverse-DNS, as dev.example.name")]
    Id(String),
    #[error("kagan {raw:?}: {source}")]
    Kagan { raw: String, source: semver::Error },
    #[error("{0}")]
    Version(#[from] semver::Error),
    #[error("needs kagan {requires}; this is {running}")]
    Needs {
        requires: semver::VersionReq,
        running: semver::Version,
    },
    #[error("entry {0:?} is not a file in the plugin")]
    Entry(String),
    #[error("commands need unique nonempty ids and titles")]
    Commands,
    #[error("a header command needs when thread")]
    HeaderCommand,
    #[error("a menu command needs when thread")]
    MenuCommand,
    #[error("$hotkeys is reserved for keyboard shortcuts")]
    ReservedHotkeys,
    #[error("tool name {0:?} must be 1 to 64 letters, digits, _ or -")]
    ToolName(String),
    #[error("tool {0} is declared twice")]
    ToolTwice(String),
    #[error("tool {name} input {input} must be a path inside the plugin", input = .input.display())]
    ToolInput { name: String, input: PathBuf },
    #[error("setting {0} needs options, and its default must be one of them")]
    Options(String),
    #[error("secret setting {key} needs a key of at most {max} bytes")]
    SecretKey { key: String, max: usize },
    #[error("unknown event {0:?}")]
    Event(String),
    #[error("thread events need the threads area")]
    EventsArea,
    #[error("run program must be a name on PATH")]
    RunProgram,
    #[error("at most {0} local fetch entries")]
    LocalFetch(usize),
    #[error("{0}")]
    Fetch(#[from] fetch::FetchError),
    #[error("theme id {0:?} must be unique and use only lowercase letters, digits and dashes")]
    ThemeId(String),
    #[error("theme {id} file {file}: {why}", file = .file.display())]
    Theme {
        id: String,
        file: PathBuf,
        why: String,
    },
    #[error("skill {} must stay inside the plugin", .0.display())]
    SkillInside(PathBuf),
    #[error("skill {} needs a SKILL.md inside the plugin", .0.display())]
    SkillMissing(PathBuf),
    #[error("skill folder needs a UTF-8 name")]
    SkillName,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    SkillParse(String),
    #[error("{0:?} is shown on the approval screen and must be one plain line")]
    PlainLine(String),
    #[error("{0}")]
    ReadRoot(#[from] ReadRootError),
    #[error("{0}")]
    Page(#[from] crate::page::Error),
    #[error("{0}")]
    Contributions(#[from] crate::contributions::Error),
    #[error("{0}")]
    CommandKey(String),
    #[error("{0}")]
    SettingField(String),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    #[serde(default)]
    pub(crate) ui: crate::contributions::Contributions,
    pub(crate) id: PluginId,
    pub(crate) name: String,
    pub(crate) version: String,
    kagan: String,
    pub(crate) entry: String,
    #[serde(default)]
    pub(crate) icon: Option<PathBuf>,
    #[serde(default)]
    pub(crate) instructions: Option<String>,
    #[serde(default)]
    pub(crate) instruct: bool,
    #[serde(default)]
    pub(crate) powers: Powers,
    #[serde(default)]
    pub(crate) settings: Settings,
    #[serde(default)]
    pub(crate) tools: Vec<Tool>,
    #[serde(default)]
    pub(crate) cli: Vec<CliGroup>,
    #[serde(default)]
    pub(crate) skills: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) commands: Vec<crate::commands::Command>,
    #[serde(default)]
    pub(crate) events: BTreeSet<String>,
    #[serde(default)]
    themes: Vec<Theme>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Theme {
    id: String,
    file: PathBuf,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CliGroup {
    pub(crate) name: String,
    pub(crate) commands: Vec<CliCommand>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CliCommand {
    pub(crate) name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    usage: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Tool {
    pub(crate) name: String,
    pub(crate) description: String,
    pub(crate) input: PathBuf,
    pub(crate) scope: Scope,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Scope {
    All,
    Tagged,
}

// Serde refuses an unknown field without naming the setting key, so the check runs on the raw JSON.
fn check_setting_fields(value: &Value) -> Result<(), Error> {
    let Some(settings) = value.get("settings").and_then(Value::as_object) else {
        return Ok(());
    };
    for (key, setting) in settings {
        let kind = setting.get("type").and_then(Value::as_str);
        if setting.get("multiline").is_some() && kind != Some("string") {
            return Err(Error::SettingField(if kind == Some("secret") {
                format!("setting {key} cannot be multiline; a secret is one line of text")
            } else {
                format!("setting {key}: multiline works on a string setting only")
            }));
        }
        if setting.get("schema").is_some() && !matches!(kind, Some("string" | "number")) {
            return Err(Error::SettingField(format!(
                "setting {key}: schema works on a string or number setting only"
            )));
        }
    }
    Ok(())
}

/// Reads and checks `kagan.json` in `root` without running plugin code.
pub(crate) fn read(root: &Path, data_dir: &Path) -> Result<Manifest, OpError> {
    let invalid = |error: Error| OpError::Invalid(format!("{FILE} in {}: {error}", root.display()));
    let text = std::fs::read_to_string(root.join(FILE))
        .map_err(|source| invalid(Error::Read { source }))?;
    let value: Value = serde_json::from_str(&text).map_err(|error| invalid(error.into()))?;
    let Value::Object(_) = &value else {
        return Err(invalid(Error::NotObject));
    };
    check_setting_fields(&value).map_err(invalid)?;
    let mut manifest: Manifest =
        serde_json::from_str(&text).map_err(|error| invalid(error.into()))?;
    manifest.check(root, data_dir).map_err(invalid)?;
    for key in manifest
        .commands
        .iter_mut()
        .filter_map(|command| command.key.as_mut())
    {
        *key = crate::commands::checked_key(key)
            .map_err(|error| invalid(Error::CommandKey(error.to_string())))?;
    }
    Ok(manifest)
}

impl Manifest {
    fn check(&self, root: &Path, data_dir: &Path) -> Result<(), Error> {
        self.ui.check()?;
        for panel in &self.ui.panels {
            if let Some(web) = panel.web.as_deref() {
                crate::page::require_html(root, web)?;
            }
        }
        self.check_id()?;
        self.check_kagan()?;
        self.check_entry(root)?;
        self.check_skills(root)?;
        self.check_commands()?;
        self.check_hotkeys()?;
        self.check_events()?;
        self.check_tools(root)?;
        self.check_options()?;
        self.check_native()?;
        self.check_fetch()?;
        self.check_read(data_dir)?;
        self.themes(root)?;
        self.shown().try_for_each(plain_line)
    }

    fn check_id(&self) -> Result<(), Error> {
        if !self.id.valid() {
            return Err(Error::Id(self.id.0.clone()));
        }
        Ok(())
    }

    fn check_kagan(&self) -> Result<(), Error> {
        let requires = semver::VersionReq::parse(&self.kagan).map_err(|source| Error::Kagan {
            raw: self.kagan.clone(),
            source,
        })?;
        let running = semver::Version::parse(env!("CARGO_PKG_VERSION"))?;
        if !requires.matches(&running) {
            return Err(Error::Needs { requires, running });
        }
        Ok(())
    }

    fn check_entry(&self, root: &Path) -> Result<(), Error> {
        if !inside(root, Path::new(&self.entry)).is_some_and(|entry| entry.is_file()) {
            return Err(Error::Entry(self.entry.clone()));
        }
        Ok(())
    }

    fn check_commands(&self) -> Result<(), Error> {
        let mut command_ids = BTreeSet::new();
        for command in &self.commands {
            if command.id.trim().is_empty()
                || command.title.trim().is_empty()
                || !command_ids.insert(&command.id)
            {
                return Err(Error::Commands);
            }
            if command.header && command.when != crate::commands::CommandWhen::Thread {
                return Err(Error::HeaderCommand);
            }
            if command.menu && command.when != crate::commands::CommandWhen::Thread {
                return Err(Error::MenuCommand);
            }
            if let Some(key) = &command.key {
                crate::commands::checked_key(key)
                    .map_err(|error| Error::CommandKey(error.to_string()))?;
            }
        }
        Ok(())
    }

    fn check_hotkeys(&self) -> Result<(), Error> {
        if self.settings.contains_key(crate::commands::BINDINGS) {
            return Err(Error::ReservedHotkeys);
        }
        Ok(())
    }

    fn check_tools(&self, root: &Path) -> Result<(), Error> {
        let mut names = BTreeSet::new();
        for tool in &self.tools {
            if !tool_name(&tool.name) {
                return Err(Error::ToolName(tool.name.clone()));
            }
            if !names.insert(&tool.name) {
                return Err(Error::ToolTwice(tool.name.clone()));
            }
            if inside(root, &tool.input).is_none() {
                return Err(Error::ToolInput {
                    name: tool.name.clone(),
                    input: tool.input.clone(),
                });
            }
        }
        Ok(())
    }

    fn check_options(&self) -> Result<(), Error> {
        for (key, setting) in self.settings.iter() {
            if let Setting::Select {
                options, default, ..
            } = setting
                && (options.is_empty()
                    || default
                        .as_ref()
                        .is_some_and(|default| !options.contains(default)))
            {
                return Err(Error::Options(key.clone()));
            }
            if let Some(schema) = setting.schema() {
                if !schema.is_object() {
                    return Err(Error::SettingField(format!(
                        "setting {key} schema must be a JSON object"
                    )));
                }
                jsonschema::validator_for(schema).map_err(|error| {
                    Error::SettingField(format!("setting {key} schema does not compile: {error}"))
                })?;
                let default = setting.default_value();
                if !default.is_null()
                    && let Some(failure) = settings::schema_failure(key, setting, &default)
                {
                    return Err(Error::SettingField(failure));
                }
            }
            if matches!(setting, Setting::Secret { .. }) && key.len() > crate::secret::MAX_KEY {
                return Err(Error::SecretKey {
                    key: key.clone(),
                    max: crate::secret::MAX_KEY,
                });
            }
        }
        Ok(())
    }

    fn check_events(&self) -> Result<(), Error> {
        for event in &self.events {
            let Some(kind) = event.strip_prefix("thread.") else {
                return Err(Error::Event(event.clone()));
            };
            if !kg_domain::EventKind::LIVE_TYPES.contains(&kind) {
                return Err(Error::Event(event.clone()));
            }
        }
        if !self.events.is_empty() && !self.powers.areas.contains(&Area::Threads) {
            return Err(Error::EventsArea);
        }
        Ok(())
    }

    fn check_native(&self) -> Result<(), Error> {
        for program in &self.powers.run {
            if program.program.is_empty() || Path::new(&program.program).components().count() != 1 {
                return Err(Error::RunProgram);
            }
        }
        Ok(())
    }

    fn check_fetch(&self) -> Result<(), Error> {
        let mut local = 0;
        for fetch in &self.powers.fetch {
            if matches!(fetch.kind()?, FetchKind::Loopback { .. }) {
                local += 1;
                if local > LOCAL_FETCH_CAP {
                    return Err(Error::LocalFetch(LOCAL_FETCH_CAP));
                }
            }
        }
        Ok(())
    }

    fn check_read(&self, data_dir: &Path) -> Result<(), Error> {
        for entry in &self.powers.read {
            read_root(entry, data_dir)?;
        }
        Ok(())
    }

    /// The declared themes, read and checked, keyed `<plugin id>/<theme id>`.
    pub(crate) fn themes(&self, root: &Path) -> Result<Vec<(String, ThemeFile)>, Error> {
        let mut ids = BTreeSet::new();
        let mut themes = Vec::new();
        for theme in &self.themes {
            if !kg_domain::valid_theme_id(&theme.id) || !ids.insert(&theme.id) {
                return Err(Error::ThemeId(theme.id.clone()));
            }
            let named = |why: String| Error::Theme {
                id: theme.id.clone(),
                file: theme.file.clone(),
                why,
            };
            let path = inside(root, &theme.file)
                .ok_or_else(|| named("must be a path inside the plugin".into()))?;
            let root = dunce::canonicalize(root).map_err(|error| named(error.to_string()))?;
            let path = dunce::canonicalize(path).map_err(|error| named(error.to_string()))?;
            if !path.starts_with(&root) {
                return Err(named("must stay inside the plugin".into()));
            }
            let file = ThemeFile::read(&path).map_err(|error| named(error.to_string()))?;
            themes.push((format!("{}/{}", self.id, theme.id), file));
        }
        Ok(themes)
    }

    fn check_skills(&self, root: &Path) -> Result<(), Error> {
        for skill in &self.skills {
            let path = inside(root, skill).ok_or_else(|| Error::SkillInside(skill.clone()))?;
            let root = dunce::canonicalize(root)?;
            let path = dunce::canonicalize(path)?;
            if !path.starts_with(&root) || !path.join("SKILL.md").is_file() {
                return Err(Error::SkillMissing(skill.clone()));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(Error::SkillName)?;
            let text = std::fs::read_to_string(path.join("SKILL.md"))?;
            kg_domain::Skill::parse(
                name,
                &text,
                kg_domain::SkillSource::Plugin,
                Some(path.join("SKILL.md")),
            )
            .map_err(|error| Error::SkillParse(error.to_string()))?;
        }
        Ok(())
    }

    /// The manifest text the approval screen shows.
    fn shown(&self) -> impl Iterator<Item = &str> {
        let powers = &self.powers;
        [self.name.as_str(), self.version.as_str()]
            .into_iter()
            .chain(powers.run.iter().map(|program| program.program.as_str()))
            .chain(powers.fetch.iter().map(|fetch| fetch.host.as_str()))
            .chain(powers.read.iter().map(String::as_str))
            .chain(self.settings.values().map(Setting::title))
    }
}

fn tool_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

fn plain_line(text: &str) -> Result<(), Error> {
    if text.chars().any(char::is_control) {
        Err(Error::PlainLine(text.to_owned()))
    } else {
        Ok(())
    }
}
