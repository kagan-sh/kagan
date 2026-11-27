//! The modes and options an agent offers a thread: its snapshot, the call that changes one, and
//! the values a new start asks for again.

use agent_client_protocol::Agent;
use agent_client_protocol::v1::ActiveSession;
use agent_client_protocol::v1::schema::{
    SessionConfigOptionValue, SetSessionConfigOptionRequest, SetSessionModeRequest,
};
use gpui_kit::{AsyncApp, WeakEntity};
use kg_domain::{MODE_CATEGORY, NoticeSeverity, ThreadId, ThreadOption};

use super::client::AcpEvent;
use super::map;
use crate::Workspace;
use crate::host::database::Result;

#[derive(Default)]
pub(super) struct Options(pub(super) Vec<ThreadOption>);

impl Options {
    // Whether the snapshot changed; only a change is written again.
    fn chosen(&mut self, id: &str, value: &str) -> bool {
        let Some(option) = self.0.iter_mut().find(|option| option.id == id) else {
            return false;
        };
        if option.value == value || !option.offers(value) {
            return false;
        }
        value.clone_into(&mut option.value);
        true
    }

    // A `current_mode_update` names the mode alone; the rest of the answer stands.
    fn mode(&mut self, id: &str) -> bool {
        self.chosen(MODE_CATEGORY, id)
    }

    // A `config_option_update` carries its own options whole, so each replaces the one it names.
    fn config(&mut self, options: &[ThreadOption]) -> bool {
        let before = self.0.clone();
        for option in options {
            match self.0.iter_mut().find(|known| known.id == option.id) {
                Some(slot) => option.clone_into(slot),
                None => self.0.push(option.clone()),
            }
        }
        self.0 != before
    }

    /// Whether the agent changed its mode or its options, so the snapshot needs writing.
    pub(super) fn changed(&mut self, update: &map::Update) -> bool {
        match update {
            map::Update::Mode(id) => self.mode(id),
            map::Update::Config(options) => self.config(options),
            _ => false,
        }
    }
}

// A mode has its own call; every other option goes through the one set-config call. kagan waits
// for the answer, so a refusal reaches the person rather than leaving the chip wrong.
pub(super) async fn choose(
    session: &mut ActiveSession<'_, Agent>,
    events: &async_channel::Sender<AcpEvent>,
    options: &mut Options,
    option: String,
    value: String,
) {
    let Some(known) = options.0.iter().find(|known| known.id == option) else {
        return;
    };
    if !known.offers(&value) {
        return;
    }
    let mode = known.is_mode();
    let session_id = session.session_id().clone();
    let sent = if mode {
        session
            .connection()
            .send_request(SetSessionModeRequest::new(session_id, value.clone()))
            .block_task()
            .await
            .map(|_| ())
    } else {
        session
            .connection()
            .send_request(SetSessionConfigOptionRequest::new(
                session_id,
                option.clone(),
                SessionConfigOptionValue::value_id(value.clone()),
            ))
            .block_task()
            .await
            .map(|_| ())
    };
    match sent {
        Ok(()) if options.chosen(&option, &value) => {
            let _ = events.send(AcpEvent::Options(options.0.clone())).await;
        }
        Ok(()) => {}
        Err(error) => {
            let _ = events
                .send(AcpEvent::Notice {
                    severity: NoticeSeverity::Warning,
                    text: format!("the agent did not change {option}: {error}"),
                })
                .await;
        }
    }
}

/// The last `options` event's list, which says what the agent offers and what the person chose.
pub(super) async fn last_options(
    this: &WeakEntity<Workspace>,
    id: &ThreadId,
    cx: &mut AsyncApp,
) -> Result<Vec<ThreadOption>> {
    let database = crate::workspace::database(this, cx)?;
    let id = id.clone();
    Ok(database
        .call(move |store| store.last_options(&id))
        .await?
        .unwrap_or_default())
}

// What a start asks for again: the recorded value of each option the agent still offers, when the
// agent came back with another.
pub(super) fn kept(remembered: &[ThreadOption], offered: &[ThreadOption]) -> Vec<(String, String)> {
    remembered
        .iter()
        .filter_map(|old| {
            let now = offered.iter().find(|option| option.id == old.id)?;
            (now.value != old.value && now.offers(&old.value))
                .then(|| (old.id.clone(), old.value.clone()))
        })
        .collect()
}
