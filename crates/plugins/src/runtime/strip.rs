use std::collections::BTreeMap;

use gpui_kit::{App, AppContext as _, Context, Entity, Global};
use gpui_shell::{HostArguments, HostError, HostValue, with_current_app};
use kg_domain::PluginId;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StatusTone {
    #[default]
    Quiet,
    Busy,
    Needs,
    Failed,
}

impl StatusTone {
    #[must_use]
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "quiet" => Some(Self::Quiet),
            "busy" => Some(Self::Busy),
            "needs" => Some(Self::Needs),
            "failed" => Some(Self::Failed),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusItem {
    pub id: PluginId,
    pub title: String,
    pub text: String,
    pub tone: StatusTone,
}

pub struct PluginStrip {
    items: BTreeMap<PluginId, StatusItem>,
}

struct PluginStripGlobal(Entity<PluginStrip>);
impl Global for PluginStripGlobal {}

impl PluginStrip {
    pub(crate) fn init(cx: &mut App) {
        let strip = cx.new(|_| Self {
            items: BTreeMap::new(),
        });
        cx.set_global(PluginStripGlobal(strip));
    }

    #[must_use]
    pub fn global(cx: &App) -> Option<Entity<Self>> {
        cx.try_global::<PluginStripGlobal>()
            .map(|global| global.0.clone())
    }

    pub fn items(&self) -> impl Iterator<Item = &StatusItem> {
        self.items.values()
    }

    pub(crate) fn set(&mut self, item: StatusItem, cx: &mut Context<Self>) {
        if self.items.get(&item.id) == Some(&item) {
            return;
        }
        self.items.insert(item.id.clone(), item);
        cx.notify();
    }

    pub(crate) fn clear(&mut self, id: &PluginId, cx: &mut Context<Self>) {
        if self.items.remove(id).is_some() {
            cx.notify();
        }
    }
}

const STATUS_CAP: usize = 120;

pub(crate) fn set_status(
    plugin: &PluginId,
    title: Option<&str>,
    secrets: &crate::secret::Store,
    arguments: &HostArguments,
) -> Result<HostValue, HostError> {
    let Some(title) = title else {
        return Err(HostError::new(format!(
            "plugin {plugin} has no status item; declare ui.status in kagan.json"
        )));
    };
    let text = match arguments.value(0)? {
        HostValue::Null => None,
        HostValue::Str(text) => Some(status_text(&secrets.redact(plugin, text))?),
        _ => {
            return Err(HostError::new(
                "setStatus needs one line of text, or null to hide it",
            ));
        }
    };
    let tone = status_tone(arguments.get(1))?;
    let item = text.map(|text| crate::StatusItem {
        id: plugin.clone(),
        title: title.to_owned(),
        text,
        tone,
    });
    with_current_app(|cx| {
        let strip = crate::PluginStrip::global(cx)
            .ok_or_else(|| HostError::new("the activity strip is unavailable"))?;
        strip.update(cx, |strip, cx| match item {
            Some(item) => strip.set(item, cx),
            None => strip.clear(plugin, cx),
        });
        Ok(HostValue::Null)
    })
    .ok_or_else(|| HostError::new("the application context is unavailable"))?
}

fn status_text(text: &str) -> Result<String, HostError> {
    if text.trim().is_empty() {
        return Err(HostError::new(
            "setStatus needs text; use null to hide the item",
        ));
    }
    let count = text.chars().count();
    if count > STATUS_CAP {
        return Err(HostError::new(format!(
            "status text is {count} characters; the cap is {STATUS_CAP}"
        )));
    }
    if text.chars().any(forbidden_status_char) {
        return Err(HostError::new(format!(
            "status text must be one plain line of at most {STATUS_CAP} characters, with no control or bidi/format characters"
        )));
    }
    Ok(text.to_owned())
}

pub(crate) fn forbidden_status_char(ch: char) -> bool {
    ch.is_control()
        || matches!(
            ch,
            '\u{061C}'
                | '\u{200B}'..='\u{200F}'
                | '\u{2028}'..='\u{202E}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{FEFF}'
        )
}

fn status_tone(options: Option<&HostValue>) -> Result<crate::StatusTone, HostError> {
    let Some(options) = options.filter(|options| !options.is_null()) else {
        return Ok(crate::StatusTone::Quiet);
    };
    let HostValue::Object(fields) = options else {
        return Err(HostError::new(
            "setStatus options must be an object with only tone",
        ));
    };
    if fields.iter().any(|(key, _)| key != "tone") {
        return Err(HostError::new(
            "setStatus options take only tone, one of quiet, busy, needs or failed",
        ));
    }
    let Some((_, value)) = fields.iter().find(|(key, _)| key == "tone") else {
        return Ok(crate::StatusTone::Quiet);
    };
    let HostValue::Str(name) = value else {
        return Err(tone_error());
    };
    crate::StatusTone::parse(name).ok_or_else(tone_error)
}

fn tone_error() -> HostError {
    HostError::new("tone must be one of quiet, busy, needs or failed")
}
