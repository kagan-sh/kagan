pub(crate) mod keys;

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{
    KeymapAction, KeymapOp, OpError, OpResult, PreferenceOp, Slot, ThemeOp, ThemeSettings,
};

use crate::host::database::{Database, Result};
use crate::workspace::closed;
use crate::{PluginRequest, Workspace};

pub(crate) async fn theme(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: ThemeOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        ThemeOp::Get => {
            let value = database.call(|store| store.setting("theme")).await?;
            let settings =
                value
                    .map(serde_json::from_value)
                    .transpose()?
                    .unwrap_or(ThemeSettings {
                        theme: "fjord".into(),
                        mode: kg_domain::Appearance::System,
                    });
            Ok(OpResult::Theme(settings))
        }
        ThemeOp::Set { theme, mode } => {
            if !kg_domain::valid_theme_choice(&theme) {
                return Err(OpError::Invalid(
                    "theme must be an id of lowercase letters, digits and dashes, or <plugin id>/<id>"
                        .into(),
                ));
            }
            let settings = ThemeSettings { theme, mode };
            let value = serde_json::to_value(&settings)?;
            database
                .call(move |store| store.set_setting("theme", &value))
                .await?;
            this.update(cx, |_, cx| {
                cx.emit(crate::WorkspaceEvent::Changed);
                cx.notify();
            })
            .map_err(closed)?;
            Ok(OpResult::Theme(settings))
        }
        ThemeOp::New { id, from } => {
            crate::dispatch::ui::drive(this, crate::UiJob::NewTheme { id, from }, cx).await
        }
        ThemeOp::Folder => {
            let dir = this
                .update(cx, |workspace, _| workspace.data_dir().join("themes"))
                .map_err(closed)?;
            cx.background_spawn(async move {
                std::fs::create_dir_all(&dir)?;
                Ok(OpResult::Files(vec![dir]))
            })
            .await
        }
    }
}

const KEYMAP: &str = "keymap";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Preference {
    ConfirmArchive,
    AgentIntegration,
    Notify,
    UpdateCheck,
    TerminalFontSize,
    SlotThreads,
    SlotReview,
    ReviewViewed,
    Sidebar,
    Width,
}

const PREFERENCES: &[(&str, Preference)] = &[
    ("confirm-archive", Preference::ConfirmArchive),
    ("agent-integration", Preference::AgentIntegration),
    ("notify", Preference::Notify),
    ("update-check", Preference::UpdateCheck),
    ("terminal-font-size", Preference::TerminalFontSize),
    (Slot::Threads.preference(), Preference::SlotThreads),
    (Slot::Review.preference(), Preference::SlotReview),
    ("review-viewed", Preference::ReviewViewed),
];
const LAYOUT: &[(&str, Preference)] = &[
    ("left-sidebar", Preference::Sidebar),
    ("left-width", Preference::Width),
    ("right-sidebar", Preference::Sidebar),
    ("right-width", Preference::Width),
    ("review-wrap", Preference::Sidebar),
];

impl Preference {
    fn parse(key: &str) -> Option<Self> {
        if let Some((_, preference)) = PREFERENCES.iter().find(|(name, _)| *name == key) {
            return Some(*preference);
        }
        let name = window_layout(key)?;
        LAYOUT
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .map(|(_, preference)| *preference)
    }

    fn accepts(self, key: &str, value: &serde_json::Value) -> Result<()> {
        match self {
            Self::TerminalFontSize => value
                .as_u64()
                .filter(|size| (9..=24).contains(size))
                .map(|_| ())
                .ok_or_else(|| {
                    OpError::Invalid("terminal-font-size must be an integer from 9 to 24".into())
                }),
            Self::ConfirmArchive => serde_json::from_value::<bool>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid("confirm-archive must be a boolean".into())),
            Self::Sidebar => serde_json::from_value::<bool>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid(format!("{key} must be a boolean"))),
            Self::Width => serde_json::from_value::<f32>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid(format!("{key} must be a number of pixels"))),
            Self::AgentIntegration => serde_json::from_value::<bool>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid("agent-integration must be a boolean".into())),
            Self::SlotThreads | Self::SlotReview => {
                serde_json::from_value::<kg_domain::SlotChoice>(value.clone())
                    .map(|_| ())
                    .map_err(|_| OpError::Invalid(format!("{key} must be a plugin id or kagan")))
            }
            Self::UpdateCheck => serde_json::from_value::<bool>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid("update-check must be a boolean".into())),
            Self::Notify => serde_json::from_value::<bool>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid("notify must be a boolean".into())),
            Self::ReviewViewed => {
                serde_json::from_value::<std::collections::BTreeMap<String, String>>(value.clone())
                    .map(|_| ())
                    .map_err(|_| OpError::Invalid("review-viewed must be a map".into()))
            }
        }
    }
}

pub(crate) async fn preference(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: PreferenceOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        PreferenceOp::Get { key } => {
            known_preference(&key)?;
            let key = key.clone();
            let value = database
                .call(move |store| store.setting(&key))
                .await?
                .unwrap_or(serde_json::Value::Null);
            Ok(OpResult::Setting(value))
        }
        PreferenceOp::Set { key, value } => {
            preference_value(&key, &value)?;
            let key = key.clone();
            let saved = value.clone();
            database
                .call(move |store| store.set_setting(&key, &saved))
                .await?;
            this.update(cx, |_, cx| {
                cx.emit(crate::WorkspaceEvent::Changed);
                cx.notify();
            })
            .map_err(closed)?;
            Ok(OpResult::Setting(value))
        }
    }
}

fn preference_value(key: &str, value: &serde_json::Value) -> Result<()> {
    Preference::parse(key)
        .ok_or_else(|| OpError::Invalid(format!("unknown preference {key}")))?
        .accepts(key, value)
}

/// The sidebar layout of one window. The layout is saved per window, as `<window>.<name>`, so
/// two windows keep their own sidebars. Returns the name, such as `left-width`.
fn window_layout(key: &str) -> Option<&str> {
    let (window, name) = key.split_once('.')?;
    let window_ok = !window.is_empty()
        && window
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (window_ok && LAYOUT.iter().any(|(candidate, _)| *candidate == name)).then_some(name)
}

fn known_preference(key: &str) -> Result<()> {
    Preference::parse(key)
        .map(|_| ())
        .ok_or_else(|| OpError::Invalid(format!("unknown preference {key}")))
}

pub(crate) async fn keymap(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: KeymapOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        KeymapOp::Get => {
            let mut value = serde_json::to_value(load_keymap(&database).await?)?;
            let OpResult::Setting(plugins) =
                crate::dispatch::forward(this, PluginRequest::Keymap(KeymapOp::Get), cx).await?
            else {
                return Err(OpError::Failed("plugin shortcuts could not be read".into()));
            };
            value["plugins"] = plugins;
            Ok(OpResult::Setting(value))
        }
        KeymapOp::Set { action, key, reset } => match KeymapAction::parse(&action)? {
            KeymapAction::Plugin { .. } => {
                let key = if reset { key } else { sequence(&key)? };
                let op = KeymapOp::Set { action, key, reset };
                crate::dispatch::forward(this, PluginRequest::Keymap(op), cx).await
            }
            KeymapAction::BuiltIn(shortcut) => {
                set_built_in(this, &database, shortcut.name, &key, reset, cx).await
            }
        },
        KeymapOp::Remove { action } => match KeymapAction::parse(&action)? {
            KeymapAction::Plugin { .. } => {
                crate::dispatch::forward(
                    this,
                    PluginRequest::Keymap(KeymapOp::Remove { action }),
                    cx,
                )
                .await
            }
            KeymapAction::BuiltIn(_) => Err(OpError::Invalid(format!(
                "{action} is built in and keeps a key: set another one, or reset it"
            ))),
        },
    }
}

async fn set_built_in(
    this: &WeakEntity<Workspace>,
    database: &Database,
    action: &str,
    key: &str,
    reset: bool,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let mut overrides = load_keymap(database).await?;
    if reset {
        overrides.reset(action);
    } else {
        let key = chord(key)?;
        if let Some(conflict) = keymap_conflict(action, &key, &overrides) {
            return Ok(OpResult::Setting(
                serde_json::json!({ "ok": false, "conflict": conflict }),
            ));
        }
        overrides.set(action.to_owned(), key);
    }
    let value = serde_json::to_value(&overrides)?;
    database
        .call(move |store| store.set_setting(KEYMAP, &value))
        .await?;
    remember_keymap(this, cx).await?;
    Ok(OpResult::Setting(serde_json::json!({ "ok": true })))
}

pub(crate) async fn remember_keymap(this: &WeakEntity<Workspace>, cx: &mut AsyncApp) -> Result<()> {
    let database = crate::workspace::database(this, cx)?;
    let overrides = load_keymap(&database).await?;
    this.update(cx, |workspace, cx| {
        workspace.set_keymap(overrides);
        cx.emit(crate::WorkspaceEvent::Changed);
        cx.notify();
    })
    .map_err(closed)?;
    Ok(())
}

async fn load_keymap(database: &Database) -> Result<kg_domain::Keymap> {
    let value = database.call(|store| store.setting(KEYMAP)).await?;
    kg_domain::Keymap::from_setting(value).map_err(|error| OpError::Failed(error.to_string()))
}

fn sequence(key: &str) -> Result<String> {
    if key.trim().is_empty() {
        return Err(OpError::Invalid("key cannot be empty".into()));
    }
    key.split_whitespace()
        .map(chord)
        .collect::<Result<Vec<_>>>()
        .map(|keys| keys.join(" "))
}

fn chord(key: &str) -> Result<String> {
    if key.trim().is_empty() {
        return Err(OpError::Invalid("key cannot be empty".into()));
    }
    let parsed = crate::parse_key(key).map_err(|error| OpError::Invalid(error.to_string()))?;
    let name = parsed.key.as_str();
    let letter_or_mark = name.chars().count() == 1
        && name
            .chars()
            .all(|ch| ch.is_alphanumeric() || ch.is_ascii_punctuation());
    let valid = letter_or_mark
        || matches!(
            name,
            "enter"
                | "tab"
                | "space"
                | "backspace"
                | "delete"
                | "escape"
                | "left"
                | "right"
                | "up"
                | "down"
                | "home"
                | "end"
                | "pageup"
                | "pagedown"
                | "insert"
        )
        || function_key(name);
    if !valid || parsed.key_char.is_some() {
        return Err(OpError::Invalid(format!("unknown key {name}")));
    }
    Ok(parsed.unparse(crate::host_os()))
}

fn function_key(name: &str) -> bool {
    name.strip_prefix('f').is_some_and(|number| {
        !number.starts_with('0')
            && number.bytes().all(|byte| byte.is_ascii_digit())
            && matches!(number.parse::<u8>(), Ok(1..=35))
    })
}

fn keymap_conflict(action: &str, key: &str, overrides: &kg_domain::Keymap) -> Option<String> {
    let key = canonical(key)?;
    let host = kg_domain::KeyPlatform::host();
    kg_domain::SHORTCUTS.iter().find_map(|shortcut| {
        if shortcut.name == action {
            return None;
        }
        let effective = overrides.binding(shortcut.name).map_or_else(
            || kg_domain::expand_secondary(shortcut.chord, host),
            str::to_owned,
        );
        (canonical(&effective).as_deref() == Some(key.as_str())).then(|| shortcut.name.to_owned())
    })
}

fn canonical(key: &str) -> Option<String> {
    crate::parse_key(key)
        .ok()
        .map(|key| key.unparse(crate::host_os()))
}

#[cfg(test)]
mod review_wrap_tests {
    use super::{LAYOUT, PREFERENCES, known_preference, preference_value};

    #[test]
    fn guide_names_every_preference() {
        let guide = include_str!("../guide-map.md");
        let behaviour = include_str!("../../../../docs/behaviour.md");
        let help = include_str!("../../../domain/src/op/mod.rs")
            .lines()
            .find(|line| line.contains("Read or save a preference:"))
            .expect("the preference help in the op modules");
        for key in PREFERENCES.iter().map(|(name, _)| *name) {
            assert!(guide.contains(&format!("`{key}`")), "guide missing {key}");
            assert!(
                behaviour.contains(&format!("`{key}`")),
                "behaviour missing {key}"
            );
            assert!(help.contains(key), "help missing {key}");
        }
        for key in LAYOUT.iter().map(|(name, _)| *name) {
            assert!(
                guide.contains(&format!("`<window>.{key}`")),
                "guide missing {key}"
            );
            assert!(
                behaviour.contains(&format!("`<window>.{key}`")),
                "behaviour missing {key}"
            );
            assert!(
                help.contains(&format!("<window>.{key}")),
                "help missing {key}"
            );
        }
    }

    #[test]
    fn review_wrap_is_a_window_boolean() {
        assert!(known_preference("main.review-wrap").is_ok());
        assert!(preference_value("main.review-wrap", &serde_json::json!(true)).is_ok());
        assert!(preference_value("main.review-wrap", &serde_json::json!("true")).is_err());
    }
}
