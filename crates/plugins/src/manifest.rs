use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use gpui_shell::HttpRequestGrant;
use indexmap::IndexMap;
use kg_domain::{OpError, PluginId, ThemeFile};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub(crate) const FILE: &str = "kagan.json";

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

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Powers {
    pub(crate) run: Vec<Program>,
    pub(crate) fetch: Vec<Fetch>,
    pub(crate) clipboard: Clipboard,
    pub(crate) areas: BTreeSet<Area>,
    pub(crate) read: Vec<String>,
    #[serde(default, rename = "openUrl")]
    pub(crate) open_url: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Program {
    pub(crate) program: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) env: Option<Env>,
}

impl Program {
    fn within(&self, approved: &Self) -> bool {
        self.program == approved.program
            && (self.env.is_none() || approved.env == Some(Env::Inherit))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Env {
    Inherit,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Fetch {
    pub(crate) host: String,
    pub(crate) methods: BTreeSet<Method>,
}

pub(crate) const LOCAL_FETCH_CAP: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FetchKind {
    Remote,
    Loopback { host: String, port: u16 },
}

impl Fetch {
    pub(crate) fn kind(&self) -> Result<FetchKind, String> {
        parse_fetch_host(&self.host)
    }

    pub(crate) fn view_host(&self) -> Result<String, String> {
        match self.kind()? {
            FetchKind::Remote => Ok(self.host.clone()),
            FetchKind::Loopback { host, .. } => Ok(host),
        }
    }

    pub(crate) fn http_grant(&self) -> Result<HttpRequestGrant, String> {
        let methods = self.methods.iter().map(|method| method.name());
        Ok(match self.kind()? {
            FetchKind::Remote => {
                HttpRequestGrant::new(self.host.clone(), methods, Vec::<String>::new(), ["/"])
            }
            FetchKind::Loopback { host, port } => {
                HttpRequestGrant::new(host, methods, Vec::<String>::new(), ["/"])
                    .scheme("http")
                    .port(port)
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub(crate) enum Method {
    #[serde(rename = "GET")]
    Get,
    #[serde(rename = "POST")]
    Post,
}

impl Method {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Get => "GET",
            Self::Post => "POST",
        }
    }
}

fn parse_fetch_host(raw: &str) -> Result<FetchKind, String> {
    if raw.eq_ignore_ascii_case("127.0.0.1") || raw.eq_ignore_ascii_case("localhost") {
        return Err(format!(
            "fetch host {raw:?} must be http://127.0.0.1:<port> or http://localhost:<port>"
        ));
    }
    if reaches_this_computer(raw) {
        return Err(
            "loopback fetch needs an approved explicit HTTP port (IPv6 is unsupported)".into(),
        );
    }
    match url::Url::parse(raw) {
        Ok(url) => {
            if url.scheme() != "http" {
                return Err(format!(
                    "fetch host {raw:?} must be a hostname, or http://127.0.0.1:<port>"
                ));
            }
            let Some(host) = url.host_str() else {
                return Err(format!("fetch host {raw:?} needs a host"));
            };
            if !host.eq_ignore_ascii_case("127.0.0.1") && !host.eq_ignore_ascii_case("localhost") {
                return Err(format!(
                    "http fetch is only for 127.0.0.1 or localhost, not {host:?}"
                ));
            }
            let port = explicit_loopback_port(raw)?;
            if url.path() != "/" && !url.path().is_empty()
                || url.query().is_some()
                || url.fragment().is_some()
                || !url.username().is_empty()
                || url.password().is_some()
            {
                return Err(format!("local fetch {raw:?} must be only http://host:port"));
            }
            Ok(FetchKind::Loopback {
                host: host.to_ascii_lowercase(),
                port,
            })
        }
        Err(_)
            if raw.contains("://")
                || raw.contains(':')
                || raw.is_empty()
                || raw.contains('/')
                || raw.contains(' ') =>
        {
            Err(format!(
                "fetch host {raw:?} must be a hostname, or http://127.0.0.1:<port>"
            ))
        }
        Err(_) => Ok(FetchKind::Remote),
    }
}

// Names that dial this computer must not become a remote HTTPS grant.
// Every IPv6 literal counts as this computer, not only IPv6 loopback.
fn reaches_this_computer(raw: &str) -> bool {
    let name = raw.trim_start_matches('[').trim_end_matches(']');
    let bare = name.trim_end_matches('.');
    let dns = bare.to_ascii_lowercase();
    if dns == "localhost" || dns.ends_with(".localhost") {
        return true;
    }
    match bare.parse::<std::net::IpAddr>() {
        Ok(std::net::IpAddr::V4(ip)) => ip.is_loopback() || ip.is_unspecified(),
        Ok(std::net::IpAddr::V6(_)) => true,
        Err(_) => false,
    }
}

// `Url::port` drops a scheme's default, so `http://127.0.0.1:80` looks unported.
fn explicit_loopback_port(raw: &str) -> Result<u16, String> {
    let Some((_, authority)) = raw.split_once("://") else {
        return Err(format!("local fetch {raw:?} needs an explicit port"));
    };
    let authority = authority
        .split_once(['/', '?', '#'])
        .map_or(authority, |(head, _)| head);
    let Some((_, port)) = authority.rsplit_once(':') else {
        return Err(format!("local fetch {raw:?} needs an explicit port"));
    };
    port.parse::<u16>()
        .map_err(|_| format!("local fetch {raw:?} needs a valid explicit port"))
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Clipboard {
    pub(crate) read: bool,
    pub(crate) write: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Area {
    Threads,
    Database,
    Process,
}

impl Area {
    pub(crate) const ALL: [Self; 3] = [Self::Threads, Self::Database, Self::Process];

    pub(crate) fn id(self) -> &'static str {
        self.row().0
    }

    pub(crate) fn name(self) -> &'static str {
        self.row().1
    }

    pub(crate) fn phrase(self) -> &'static str {
        self.row().2
    }

    fn row(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Threads => (
                "threads",
                "Threads",
                "reads, starts and changes your threads, can type into threads and press Enter, reads their screen text, and reads their changed files",
            ),
            Self::Database => ("database", "Database", "keeps its own database"),
            Self::Process => (
                "process",
                "Process",
                "starts and stops the programs listed here",
            ),
        }
    }
}

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
        #[serde(default)]
        multiline: bool,
        #[serde(default)]
        schema: Option<Value>,
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
        #[serde(default)]
        schema: Option<Value>,
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
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

    pub(crate) fn schema(&self) -> Option<&Value> {
        match self {
            Self::String { schema, .. } | Self::Number { schema, .. } => schema.as_ref(),
            _ => None,
        }
    }
}

/// The first way `value` fails `setting`'s schema, in plain words. `None` when it passes or the
/// setting declares no schema.
pub(crate) fn schema_failure(key: &str, setting: &Setting, value: &Value) -> Option<String> {
    let schema = setting.schema()?;
    let validator = jsonschema::validator_for(schema).ok()?;
    let failure = validator.iter_errors(value).next()?;
    Some(format!(
        "setting {key} ({}) does not match its schema: {failure}",
        setting.title()
    ))
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

/// Refuses `multiline` and `schema` on a setting type that does not take them, naming the key.
/// Serde would refuse them as unknown fields, without the key, so this runs on the raw JSON.
fn check_setting_fields(value: &Value) -> Result<(), String> {
    let Some(settings) = value.get("settings").and_then(Value::as_object) else {
        return Ok(());
    };
    for (key, setting) in settings {
        let kind = setting.get("type").and_then(Value::as_str);
        if setting.get("multiline").is_some() && kind != Some("string") {
            return Err(if kind == Some("secret") {
                format!("setting {key} cannot be multiline; a secret is one line of text")
            } else {
                format!("setting {key}: multiline works on a string setting only")
            });
        }
        if setting.get("schema").is_some() && !matches!(kind, Some("string" | "number")) {
            return Err(format!(
                "setting {key}: schema works on a string or number setting only"
            ));
        }
    }
    Ok(())
}

/// Reads and checks `kagan.json` in `root` without running plugin code.
pub(crate) fn read(root: &Path, data_dir: &Path) -> Result<Manifest, OpError> {
    let invalid =
        |message: String| OpError::Invalid(format!("{FILE} in {}: {message}", root.display()));
    let text = std::fs::read_to_string(root.join(FILE))
        .map_err(|error| invalid(format!("cannot read it: {error}")))?;
    let value: Value = serde_json::from_str(&text).map_err(|error| invalid(error.to_string()))?;
    let Value::Object(_) = &value else {
        return Err(invalid("must be a JSON object".into()));
    };
    check_setting_fields(&value).map_err(invalid)?;
    let mut manifest: Manifest =
        serde_json::from_str(&text).map_err(|error| invalid(error.to_string()))?;
    manifest.check(root, data_dir).map_err(invalid)?;
    for key in manifest
        .commands
        .iter_mut()
        .filter_map(|command| command.key.as_mut())
    {
        *key = crate::commands::checked_key(key).map_err(|error| invalid(error.to_string()))?;
    }
    Ok(manifest)
}

impl Manifest {
    fn check(&self, root: &Path, data_dir: &Path) -> Result<(), String> {
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

    fn check_id(&self) -> Result<(), String> {
        if !self.id.valid() {
            return Err(format!(
                "id {:?} must be reverse-DNS, as dev.example.name",
                self.id.0
            ));
        }
        Ok(())
    }

    fn check_kagan(&self) -> Result<(), String> {
        let requires = semver::VersionReq::parse(&self.kagan)
            .map_err(|error| format!("kagan {:?}: {error}", self.kagan))?;
        let running =
            semver::Version::parse(env!("CARGO_PKG_VERSION")).map_err(|error| error.to_string())?;
        if !requires.matches(&running) {
            return Err(format!("needs kagan {requires}; this is {running}"));
        }
        Ok(())
    }

    fn check_entry(&self, root: &Path) -> Result<(), String> {
        if !inside(root, Path::new(&self.entry)).is_some_and(|entry| entry.is_file()) {
            return Err(format!(
                "entry {:?} is not a file in the plugin",
                self.entry
            ));
        }
        Ok(())
    }

    fn check_commands(&self) -> Result<(), String> {
        let mut command_ids = BTreeSet::new();
        for command in &self.commands {
            if command.id.trim().is_empty()
                || command.title.trim().is_empty()
                || !command_ids.insert(&command.id)
            {
                return Err("commands need unique nonempty ids and titles".into());
            }
            if command.header && command.when != crate::commands::CommandWhen::Thread {
                return Err("a header command needs when thread".into());
            }
            if let Some(key) = &command.key {
                crate::commands::checked_key(key).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    fn check_hotkeys(&self) -> Result<(), String> {
        if self.settings.contains_key(crate::commands::BINDINGS) {
            return Err("$hotkeys is reserved for keyboard shortcuts".into());
        }
        Ok(())
    }

    fn check_tools(&self, root: &Path) -> Result<(), String> {
        let mut names = BTreeSet::new();
        for tool in &self.tools {
            if !tool_name(&tool.name) {
                return Err(format!(
                    "tool name {:?} must be 1 to 64 letters, digits, _ or -",
                    tool.name
                ));
            }
            if !names.insert(&tool.name) {
                return Err(format!("tool {} is declared twice", tool.name));
            }
            if inside(root, &tool.input).is_none() {
                return Err(format!(
                    "tool {} input {} must be a path inside the plugin",
                    tool.name,
                    tool.input.display()
                ));
            }
        }
        Ok(())
    }

    fn check_options(&self) -> Result<(), String> {
        for (key, setting) in self.settings.iter() {
            if let Setting::Select {
                options, default, ..
            } = setting
                && (options.is_empty()
                    || default
                        .as_ref()
                        .is_some_and(|default| !options.contains(default)))
            {
                return Err(format!(
                    "setting {key} needs options, and its default must be one of them"
                ));
            }
            if matches!(setting, Setting::Secret { .. }) && key.len() > crate::secret::MAX_KEY {
                return Err(format!(
                    "secret setting {key} needs a key of at most {} bytes",
                    crate::secret::MAX_KEY
                ));
            }
            if let Some(schema) = setting.schema() {
                if !schema.is_object() {
                    return Err(format!("setting {key} schema must be a JSON object"));
                }
                jsonschema::validator_for(schema)
                    .map_err(|error| format!("setting {key} schema does not compile: {error}"))?;
                let default = setting.default_value();
                if !default.is_null()
                    && let Some(failure) = schema_failure(key, setting, &default)
                {
                    return Err(failure);
                }
            }
        }
        Ok(())
    }

    fn check_events(&self) -> Result<(), String> {
        for event in &self.events {
            let Some(kind) = event.strip_prefix("thread.") else {
                return Err(format!("unknown event {event:?}"));
            };
            if !kg_domain::EventKind::LIVE_TYPES.contains(&kind) {
                return Err(format!("unknown event {event:?}"));
            }
        }
        if !self.events.is_empty() && !self.powers.areas.contains(&Area::Threads) {
            return Err("thread events need the threads area".into());
        }
        Ok(())
    }

    fn check_native(&self) -> Result<(), String> {
        for program in &self.powers.run {
            if program.program.is_empty() || Path::new(&program.program).components().count() != 1 {
                return Err("run program must be a name on PATH".into());
            }
        }
        Ok(())
    }

    fn check_fetch(&self) -> Result<(), String> {
        let mut local = 0;
        for fetch in &self.powers.fetch {
            if matches!(fetch.kind()?, FetchKind::Loopback { .. }) {
                local += 1;
                if local > LOCAL_FETCH_CAP {
                    return Err(format!("at most {LOCAL_FETCH_CAP} local fetch entries"));
                }
            }
        }
        Ok(())
    }

    fn check_read(&self, data_dir: &Path) -> Result<(), String> {
        for entry in &self.powers.read {
            read_root(entry, data_dir)?;
        }
        Ok(())
    }

    /// The declared themes, read and checked, keyed `<plugin id>/<theme id>`.
    pub(crate) fn themes(&self, root: &Path) -> Result<Vec<(String, ThemeFile)>, String> {
        let mut ids = BTreeSet::new();
        let mut themes = Vec::new();
        for theme in &self.themes {
            if !kg_domain::valid_theme_id(&theme.id) || !ids.insert(&theme.id) {
                return Err(format!(
                    "theme id {:?} must be unique and use only lowercase letters, digits and dashes",
                    theme.id
                ));
            }
            let named = |message: String| {
                format!(
                    "theme {} file {}: {message}",
                    theme.id,
                    theme.file.display()
                )
            };
            let path = inside(root, &theme.file)
                .ok_or_else(|| named("must be a path inside the plugin".into()))?;
            let root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
            let path = dunce::canonicalize(path).map_err(|error| named(error.to_string()))?;
            if !path.starts_with(&root) {
                return Err(named("must stay inside the plugin".into()));
            }
            let file = ThemeFile::read(&path).map_err(|error| named(error.to_string()))?;
            themes.push((format!("{}/{}", self.id, theme.id), file));
        }
        Ok(themes)
    }

    fn check_skills(&self, root: &Path) -> Result<(), String> {
        for skill in &self.skills {
            let path = inside(root, skill)
                .ok_or_else(|| format!("skill {} must stay inside the plugin", skill.display()))?;
            let root = dunce::canonicalize(root).map_err(|error| error.to_string())?;
            let path = dunce::canonicalize(path).map_err(|error| error.to_string())?;
            if !path.starts_with(&root) || !path.join("SKILL.md").is_file() {
                return Err(format!(
                    "skill {} needs a SKILL.md inside the plugin",
                    skill.display()
                ));
            }
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| "skill folder needs a UTF-8 name".to_owned())?;
            let text = std::fs::read_to_string(path.join("SKILL.md"))
                .map_err(|error| error.to_string())?;
            kg_domain::Skill::parse(
                name,
                &text,
                kg_domain::SkillSource::Plugin,
                Some(path.join("SKILL.md")),
            )
            .map_err(|error| error.to_string())?;
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

fn plain_line(text: &str) -> Result<(), String> {
    if text.chars().any(char::is_control) {
        Err(format!(
            "{text:?} is shown on the approval screen and must be one plain line"
        ))
    } else {
        Ok(())
    }
}

pub(crate) fn read_root(entry: &str, data_dir: &Path) -> Result<Option<PathBuf>, String> {
    let rest = entry
        .strip_prefix("~/")
        .ok_or_else(|| format!("read path {entry:?} must be a folder under home, written as ~/"))?;
    if rest.is_empty()
        || rest.contains('\\')
        || rest.contains(':')
        || !Path::new(rest)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(format!(
            "read path {entry:?} must stay inside the home directory"
        ));
    }
    let home = home_dir()?;
    let joined = home.join(rest);
    if !joined.starts_with(&home) {
        return Err(format!(
            "read path {entry:?} resolves outside the home directory"
        ));
    }
    if let Some(error) = denied(&joined, entry, data_dir, &home) {
        return Err(error);
    }
    if !joined.exists() {
        return Ok(None);
    }
    if !joined.is_dir() {
        return Err(format!("read path {entry:?} must be a folder"));
    }
    let canonical = dunce::canonicalize(&joined).map_err(|error| error.to_string())?;
    let home = match dunce::canonicalize(&home) {
        Ok(path) => path,
        Err(_) => home,
    };
    if let Some(error) = denied(&canonical, entry, data_dir, &home) {
        return Err(error);
    }
    if canonical == home || !canonical.starts_with(&home) {
        return Err(format!(
            "read path {entry:?} resolves outside the home directory"
        ));
    }
    Ok(Some(canonical))
}

fn denied(path: &Path, entry: &str, data_dir: &Path, home: &Path) -> Option<String> {
    let ssh = home.join(".ssh");
    let gnupg = home.join(".gnupg");
    let protected = [
        (data_dir, resolved(data_dir).display().to_string()),
        (ssh.as_path(), "~/.ssh".to_owned()),
        (gnupg.as_path(), "~/.gnupg".to_owned()),
    ];
    protected.into_iter().find_map(|(protected, name)| {
        covers(path, protected).then(|| format!("read path {entry:?} cannot cover {name}"))
    })
}

fn covers(path: &Path, protected: &Path) -> bool {
    let path_resolved = resolved(path);
    let protected_resolved = resolved(protected);
    path_resolved.starts_with(&protected_resolved)
        || protected_resolved.starts_with(&path_resolved)
        || path.starts_with(protected)
        || protected.starts_with(path)
}

fn resolved(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn home_dir() -> Result<PathBuf, String> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .ok_or_else(|| format!("{key} is not set"))
}

/// `relative` joined to `root`, if it stays inside it.
pub(crate) fn inside(root: &Path, relative: &Path) -> Option<PathBuf> {
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        .then(|| root.join(relative))
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Grant {
    #[serde(default)]
    pub(crate) powers: Powers,
    #[serde(default)]
    pub(crate) folders: BTreeMap<String, String>,
    /// The folder a person approved for each folder setting, canonical.
    #[serde(default)]
    pub(crate) paths: BTreeMap<String, PathBuf>,
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
