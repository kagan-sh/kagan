use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{KeymapAction, KeymapOp, OpError, OpResult, PreferenceOp, ThemeOp, ThemeSettings};

use crate::database::{Database, Result};
use crate::workspace::closed;
use crate::{PluginRequest, Workspace};

pub(super) async fn theme(
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
            crate::ui::drive(this, crate::UiJob::NewTheme { id, from }, cx).await
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
const PREFERENCES: &[&str] = &[
    "confirm-archive",
    "agent-integration",
    "notify",
    "terminal-font-size",
    "slot.threads",
    "slot.review",
];
const LAYOUT: &[&str] = &[
    "left-sidebar",
    "left-width",
    "right-sidebar",
    "right-width",
    "review-wrap",
];

pub(super) async fn preference(
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
            known_preference(&key)?;
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
    match window_layout(key).unwrap_or(key) {
        "terminal-font-size" => value
            .as_u64()
            .filter(|size| (9..=24).contains(size))
            .map(|_| ())
            .ok_or_else(|| {
                OpError::Invalid("terminal-font-size must be an integer from 9 to 24".into())
            }),
        "confirm-archive" => serde_json::from_value::<bool>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid("confirm-archive must be a boolean".into())),
        "left-sidebar" | "right-sidebar" | "review-wrap" => {
            serde_json::from_value::<bool>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid(format!("{key} must be a boolean")))
        }
        "left-width" | "right-width" => serde_json::from_value::<f32>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid(format!("{key} must be a number of pixels"))),
        "agent-integration" => serde_json::from_value::<bool>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid("agent-integration must be a boolean".into())),
        "slot.threads" | "slot.review" => {
            serde_json::from_value::<kg_domain::SlotChoice>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid(format!("{key} must be a plugin id or kagan")))
        }
        "notify" => serde_json::from_value::<bool>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid("notify must be a boolean".into())),
        "review-viewed" => {
            serde_json::from_value::<std::collections::BTreeMap<String, String>>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid("review-viewed must be a map".into()))
        }
        _ if panel_key(key) => serde_json::from_value::<bool>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid(format!("{key} must be a boolean"))),
        _ => Err(OpError::Invalid(format!("unknown preference {key}"))),
    }
}

/// The sidebar layout of one window. The layout is saved per window, as `<window>.<name>`, so
/// two windows keep their own sidebars. Returns the name, such as `left-width`.
fn window_layout(key: &str) -> Option<&str> {
    let (window, name) = key.split_once('.')?;
    let window_ok = !window.is_empty()
        && window
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (window_ok && LAYOUT.contains(&name)).then_some(name)
}

/// A right panel's saved collapse, as `panel.<id>.collapsed`.
fn panel_key(key: &str) -> bool {
    key.strip_prefix("panel.")
        .and_then(|rest| rest.strip_suffix(".collapsed"))
        .is_some_and(|id| !id.is_empty())
}

fn known_preference(key: &str) -> Result<()> {
    if PREFERENCES.contains(&key)
        || key == "review-viewed"
        || window_layout(key).is_some()
        || panel_key(key)
    {
        Ok(())
    } else {
        Err(OpError::Invalid(format!("unknown preference {key}")))
    }
}

pub(super) async fn keymap(
    this: &WeakEntity<Workspace>,
    database: Database,
    op: KeymapOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    match op {
        KeymapOp::Get => {
            let mut value = serde_json::to_value(load_keymap(&database).await?)?;
            let OpResult::Setting(plugins) =
                super::forward(this, PluginRequest::Keymap(KeymapOp::Get), cx).await?
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
                super::forward(this, PluginRequest::Keymap(op), cx).await
            }
            KeymapAction::BuiltIn(shortcut) => {
                set_built_in(this, &database, shortcut.name, &key, reset, cx).await
            }
        },
        KeymapOp::Remove { action } => match KeymapAction::parse(&action)? {
            KeymapAction::Plugin { .. } => {
                super::forward(this, PluginRequest::Keymap(KeymapOp::Remove { action }), cx).await
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
    let database = this
        .update(cx, |workspace, _| workspace.database.clone())
        .map_err(closed)?;
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
        for key in PREFERENCES
            .iter()
            .copied()
            .chain(["review-viewed", "panel.<id>.collapsed"])
        {
            assert!(guide.contains(&format!("`{key}`")), "missing {key}");
        }
        for key in LAYOUT {
            assert!(
                guide.contains(&format!("`<window>.{key}`")),
                "missing {key}"
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
