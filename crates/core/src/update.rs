//! Finds, downloads, checks and installs a newer kagan (decision 113).

mod install;

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine as _;
use gpui_kit::{App, AppContext as _, AsyncApp, Context, Entity, Global, Subscription, Task};
use kg_domain::{Install, OpError, OpResult, UpdateOp, UpdateState, UpdateStatus};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Deserialize;

use crate::database::{Database, Result};
pub use install::Target;

pub(crate) const PREFERENCE: &str = "update-check";

// A test build can name its own key. Nothing at runtime can replace it.
const KEY: &str = match option_env!("KG_UPDATE_KEY") {
    Some(key) => key,
    None => "RWSUEdbT8kzFInVNdQBN8fW1I4JQT5JiTj3TBCWZOlDbV7Pttvn2cEKu",
};
const MANIFEST: &str = "https://github.com/kagan-sh/kagan/releases/latest/download/update.json";
const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const TICK: Duration = Duration::from_secs(60 * 60);

#[derive(Deserialize)]
struct Manifest {
    version: String,
    notes: String,
    platforms: HashMap<String, Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    url: String,
    signature: String,
}

struct UpdaterGlobal(Entity<Updater>);
impl Global for UpdaterGlobal {}

pub struct Updater {
    version: Version,
    target: Target,
    endpoint: String,
    key: String,
    state: UpdateState,
    asset: Option<Asset>,
    staged: Option<PathBuf>,
    at_quit: bool,
    restart: bool,
    checked: Option<Instant>,
    _automatic: Task<()>,
    _quit: Subscription,
}

impl Updater {
    pub(crate) fn install(database: Database, cx: &mut App) {
        let updater = cx.new(|cx: &mut Context<Self>| {
            let target = Target::detect();
            let cleanup = target.clone();
            cx.background_spawn(async move { cleanup.clean() }).detach();
            Self {
                version: current(),
                target,
                endpoint: std::env::var("KG_UPDATE_URL").unwrap_or_else(|_| MANIFEST.into()),
                key: KEY.into(),
                state: UpdateState::Unchecked,
                asset: None,
                staged: None,
                at_quit: false,
                restart: false,
                checked: None,
                _automatic: cx.spawn(async move |this, cx| automatic(this, database, cx).await),
                _quit: cx.on_app_quit(Self::quit),
            }
        });
        cx.set_global(UpdaterGlobal(updater));
    }

    #[must_use]
    pub fn global(cx: &App) -> Entity<Self> {
        cx.global::<UpdaterGlobal>().0.clone()
    }

    #[must_use]
    pub fn state(&self) -> &UpdateState {
        &self.state
    }

    #[must_use]
    pub fn kind(&self) -> Install {
        self.target.install
    }

    #[must_use]
    pub fn version(&self) -> String {
        self.version.to_string()
    }

    pub(crate) fn status(&self, automatic: bool) -> UpdateStatus {
        UpdateStatus {
            version: self.version(),
            install: self.target.install,
            automatic,
            state: self.state.clone(),
        }
    }

    /// Reads the manifest. A version that is not newer, or is a pre-release, is not offered.
    pub fn check(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        if matches!(
            self.state,
            UpdateState::Checking | UpdateState::Downloading { .. } | UpdateState::Ready { .. }
        ) {
            return Task::ready(Ok(()));
        }
        self.set(UpdateState::Checking, cx);
        self.checked = Some(cx.background_executor().now());
        let endpoint = self.endpoint.clone();
        let fetch = cx.background_spawn(async move { manifest(&endpoint) });
        cx.spawn(async move |this, cx| {
            let found = fetch.await;
            this.update(cx, |this, cx| match found {
                Ok(manifest) => {
                    let offered = newer(&this.version, &manifest.version)
                        .zip(manifest.platforms.get(&platform()).cloned());
                    let state = match offered {
                        Some((version, asset)) => {
                            this.asset = Some(asset);
                            UpdateState::Available {
                                version: version.to_string(),
                                notes: manifest.notes,
                            }
                        }
                        None => UpdateState::Current,
                    };
                    this.set(state, cx);
                    Ok(())
                }
                Err(reason) => {
                    this.set(
                        UpdateState::Failed {
                            reason: reason.clone(),
                        },
                        cx,
                    );
                    Err(OpError::Failed(reason))
                }
            })
            .map_err(|_| OpError::Failed("kagan is quitting".into()))?
        })
    }

    /// Downloads the offered version, checks its signature and puts it beside this copy.
    /// Not an `Op`: only the person installs, from the window.
    pub fn download(&mut self, at_quit: bool, cx: &mut Context<Self>) -> Task<Result<()>> {
        let UpdateState::Available { version, notes } = self.state.clone() else {
            return Task::ready(Err(OpError::Invalid("no update is waiting".into())));
        };
        let Some(asset) = self.asset.clone() else {
            return Task::ready(Err(OpError::Invalid("no update is waiting".into())));
        };
        if !self.target.install.replaces_itself() {
            return Task::ready(Err(OpError::Invalid(
                "this copy of kagan is updated outside kagan".into(),
            )));
        }
        self.set(
            UpdateState::Downloading {
                version: version.clone(),
            },
            cx,
        );
        let key = self.key.clone();
        let target = self.target.clone();
        let work = cx.background_spawn(async move {
            let bytes = get(&asset.url)?;
            verify(&bytes, &asset.signature, &key)?;
            target.stage(&bytes).map_err(|error| error.to_string())
        });
        cx.spawn(async move |this, cx| {
            let staged = work.await;
            this.update(cx, |this, cx| match staged {
                Ok(path) => {
                    this.staged = Some(path);
                    this.at_quit = at_quit;
                    this.set(UpdateState::Ready { version, notes }, cx);
                    Ok(())
                }
                Err(reason) => {
                    this.set(UpdateState::Available { version, notes }, cx);
                    Err(OpError::Failed(reason))
                }
            })
            .map_err(|_| OpError::Failed("kagan is quitting".into()))?
        })
    }

    /// Puts the downloaded copy in place of this one, and starts it when kagan quits. The
    /// Windows installer runs only after kagan has quit.
    pub fn replace(&mut self, cx: &mut Context<Self>) -> Task<Result<()>> {
        let Some(staged) = self.staged.clone() else {
            return Task::ready(Err(OpError::Invalid("no update is downloaded".into())));
        };
        let target = self.target.clone();
        let work = cx.background_spawn(async move {
            if target.install == Install::Installer {
                return Ok(());
            }
            target.replace(&staged)
        });
        cx.spawn(async move |this, cx| {
            let replaced = work.await;
            this.update(cx, |this, _| {
                replaced.map_err(|error| OpError::Failed(error.to_string()))?;
                this.staged = None;
                this.restart = true;
                Ok(())
            })
            .map_err(|_| OpError::Failed("kagan is quitting".into()))?
        })
    }

    fn quit(&mut self, cx: &mut Context<Self>) -> impl Future<Output = ()> + use<> {
        let target = self.target.clone();
        let staged = self.staged.take().filter(|_| self.at_quit);
        let restart = self.restart;
        cx.background_spawn(async move {
            if let Some(staged) = staged
                && let Err(error) = target.replace(&staged)
            {
                eprintln!("kg: could not install the update: {error}");
            }
            if restart && let Err(error) = target.relaunch() {
                eprintln!("kg: could not start kagan again: {error}");
            }
        })
    }

    fn set(&mut self, state: UpdateState, cx: &mut Context<Self>) {
        self.state = state;
        cx.notify();
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn use_state(&mut self, state: UpdateState, target: Target, cx: &mut Context<Self>) {
        self.target = target;
        self.set(state, cx);
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn use_source(&mut self, endpoint: String, key: String, target: Target) {
        self.endpoint = endpoint;
        self.key = key;
        self.target = target;
    }
}

pub(crate) async fn run(database: &Database, op: UpdateOp, cx: &mut AsyncApp) -> Result<OpResult> {
    let updater = cx.update(|cx| Updater::global(cx));
    if op == UpdateOp::Check {
        updater.update(cx, |updater, cx| updater.check(cx)).await?;
    }
    let automatic = automatic_on(database).await;
    Ok(OpResult::Update(
        updater.read_with(cx, |updater, _| updater.status(automatic)),
    ))
}

async fn automatic_on(database: &Database) -> bool {
    database
        .call(|store| store.setting(PREFERENCE))
        .await
        .ok()
        .flatten()
        .and_then(|value| value.as_bool())
        .unwrap_or(false)
}

async fn automatic(this: gpui_kit::WeakEntity<Updater>, database: Database, cx: &mut AsyncApp) {
    loop {
        let on = automatic_on(&database).await;
        let now = cx.background_executor().now();
        let checked = this.update(cx, |this, cx| {
            let due = this
                .checked
                .is_none_or(|checked| now.duration_since(checked) >= DAY);
            (on && due).then(|| this.check(cx))
        });
        match checked {
            Ok(Some(task)) => drop(task.await),
            Ok(None) => {}
            Err(_) => return,
        }
        cx.background_executor().timer(TICK).await;
    }
}

fn current() -> Version {
    Version::parse(env!("CARGO_PKG_VERSION")).unwrap_or_else(|_| Version::new(0, 0, 0))
}

fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

pub(crate) fn newer(current: &Version, offered: &str) -> Option<Version> {
    let offered = Version::parse(offered.trim_start_matches('v')).ok()?;
    (offered.pre.is_empty() && offered > *current).then_some(offered)
}

/// `signature` is the `.sig` file cargo-packager writes: a minisign signature, base64 encoded.
pub(crate) fn verify(bytes: &[u8], signature: &str, key: &str) -> std::result::Result<(), String> {
    let key = PublicKey::from_base64(key).map_err(|error| format!("bad public key: {error}"))?;
    let text = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|text| String::from_utf8(text).ok())
        .ok_or("the update has no readable signature")?;
    let signature =
        Signature::decode(&text).map_err(|_| "the update has no readable signature".to_owned())?;
    key.verify(bytes, &signature, false)
        .map_err(|_| "the update's signature does not match kagan's key".to_owned())
}

fn client() -> std::result::Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(concat!("kagan/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| error.to_string())
}

fn get(url: &str) -> std::result::Result<Vec<u8>, String> {
    let response = client()?
        .get(url)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| format!("could not reach {url}: {error}"))?;
    response
        .bytes()
        .map(|bytes| bytes.to_vec())
        .map_err(|error| format!("could not download {url}: {error}"))
}

fn manifest(url: &str) -> std::result::Result<Manifest, String> {
    let bytes = get(url)?;
    serde_json::from_slice(&bytes).map_err(|error| format!("the update list is not valid: {error}"))
}
