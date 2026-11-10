pub(crate) mod keys;

use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{
    KeymapAction, KeymapOp, OpError, OpResult, PreferenceOp, Slot, ThemeOp, ThemeSettings, prefs,
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
            let settings = value
                .map(serde_json::from_value)
                .transpose()?
                .unwrap_or_default();
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
            crate::workspace::changed(this, cx)?;
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
enum Kind {
    Flag,
    Notify,
    FontSize,
    Slot,
    Pixels,
    Map,
    BranchPrefix,
    Density,
    FontName,
}

const PREFERENCES: &[(&str, Kind)] = &[
    (prefs::CONFIRM_ARCHIVE, Kind::Flag),
    (prefs::AGENT_INTEGRATION, Kind::Flag),
    (prefs::NOTIFY, Kind::Notify),
    (prefs::PLUGIN_STATUS, Kind::Flag),
    (prefs::UPDATE_CHECK, Kind::Flag),
    (prefs::SETUP_GUIDE, Kind::Flag),
    (prefs::TERMINAL_FONT_SIZE, Kind::FontSize),
    (prefs::BRANCH_PREFIX, Kind::BranchPrefix),
    (prefs::TERMINAL_COPY_ON_SELECT, Kind::Flag),
    (prefs::TERMINAL_LINKS_BROWSER, Kind::Flag),
    (prefs::DENSITY, Kind::Density),
    (prefs::FONT_UI, Kind::FontName),
    (prefs::FONT_TERMINAL, Kind::FontName),
    (Slot::Threads.preference(), Kind::Slot),
    (Slot::Review.preference(), Kind::Slot),
    (prefs::REVIEW_VIEWED, Kind::Map),
];
const LAYOUT: &[(&str, Kind)] = &[
    (prefs::LEFT_SIDEBAR, Kind::Flag),
    (prefs::LEFT_WIDTH, Kind::Pixels),
    (prefs::RIGHT_SIDEBAR, Kind::Flag),
    (prefs::RIGHT_WIDTH, Kind::Pixels),
    (prefs::REVIEW_WRAP, Kind::Flag),
];

fn kind(key: &str) -> Option<Kind> {
    if let Some((_, kind)) = PREFERENCES.iter().find(|(name, _)| *name == key) {
        return Some(*kind);
    }
    let name = window_layout(key)?;
    LAYOUT
        .iter()
        .find(|(candidate, _)| *candidate == name)
        .map(|(_, kind)| *kind)
}

fn accepts(kind: Kind, key: &str, value: &serde_json::Value) -> Result<()> {
    match kind {
        Kind::Flag => serde_json::from_value::<bool>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid(format!("{key} must be a boolean"))),
        Kind::FontSize => value
            .as_u64()
            .and_then(|size| u8::try_from(size).ok())
            .filter(|size| (prefs::TERMINAL_FONT_MIN..=prefs::TERMINAL_FONT_MAX).contains(size))
            .map(|_| ())
            .ok_or_else(|| {
                OpError::Invalid(format!(
                    "{key} must be an integer from {} to {}",
                    prefs::TERMINAL_FONT_MIN,
                    prefs::TERMINAL_FONT_MAX
                ))
            }),
        Kind::Slot => serde_json::from_value::<kg_domain::SlotChoice>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid(format!("{key} must be a plugin id or kagan"))),
        Kind::Notify => serde_json::from_value::<kg_domain::NotifyLevel>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid(format!("{key} must be all, asks or off"))),
        Kind::Pixels => serde_json::from_value::<f32>(value.clone())
            .map(|_| ())
            .map_err(|_| OpError::Invalid(format!("{key} must be a number of pixels"))),
        Kind::FontName => value
            .as_str()
            .filter(|text| prefs::valid_font_name(text))
            .map(|_| ())
            .ok_or_else(|| {
                OpError::Invalid(format!(
                    "{key} must be a font family name of up to {} bytes, or empty",
                    prefs::FONT_NAME_MAX
                ))
            }),
        Kind::Density => value
            .as_str()
            .filter(|text| prefs::DENSITIES.contains(text))
            .map(|_| ())
            .ok_or_else(|| {
                OpError::Invalid(format!("{key} must be compact, default or roomy"))
            }),
        Kind::BranchPrefix => value
            .as_str()
            .filter(|text| prefs::valid_branch_prefix(text))
            .map(|_| ())
            .ok_or_else(|| {
                OpError::Invalid(format!(
                    "{key} must be empty, or up to {} bytes and a valid git branch start such as bb/",
                    prefs::BRANCH_PREFIX_MAX
                ))
            }),
        Kind::Map => {
            serde_json::from_value::<std::collections::BTreeMap<String, String>>(value.clone())
                .map(|_| ())
                .map_err(|_| OpError::Invalid(format!("{key} must be a map")))
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
            crate::workspace::changed(this, cx)?;
            Ok(OpResult::Setting(value))
        }
    }
}

fn preference_value(key: &str, value: &serde_json::Value) -> Result<()> {
    let kind = kind(key).ok_or_else(|| OpError::Invalid(format!("unknown preference {key}")))?;
    accepts(kind, key, value)
}

// The sidebar layout of one window. The layout is saved per window, as `<window>.<name>`, so
// two windows keep their own sidebars. Returns the name, such as `left-width`.
fn window_layout(key: &str) -> Option<&str> {
    let (window, name) = key.split_once('.')?;
    let window_ok = !window.is_empty()
        && window
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    (window_ok && LAYOUT.iter().any(|(candidate, _)| *candidate == name)).then_some(name)
}

fn known_preference(key: &str) -> Result<()> {
    kind(key)
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
            KeymapAction::BuiltIn(shortcut) => {
                let mut overrides = load_keymap(&database).await?;
                overrides.unbind(shortcut.name.to_owned());
                store_keymap(this, &database, &overrides, cx).await
            }
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
    store_keymap(this, database, &overrides, cx).await
}

async fn store_keymap(
    this: &WeakEntity<Workspace>,
    database: &Database,
    overrides: &kg_domain::Keymap,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let value = serde_json::to_value(overrides)?;
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
#[path = "review_wrap_test.rs"]
mod review_wrap_tests;
