//! Finds, downloads, checks and installs a newer kagan (decisions 113 to 116).

mod install;

use std::collections::HashMap;
use std::io::Read as _;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use base64::Engine as _;
use gpui_kit::{App, AppContext as _, AsyncApp, Context, Entity, Global, Subscription, Task};
use kg_domain::{InstallKind, OpError, OpResult, UpdateOp, UpdateState, UpdateStatus};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::Deserialize;

use crate::host::database::{Database, Result};
pub use install::Target;

pub(crate) const PREFERENCE: &str = "update-check";

// A test build can name its own key. Nothing at runtime can replace it.
const KEY: &str = match option_env!("KG_UPDATE_KEY") {
    Some(key) => key,
    None => "RWSUEdbT8kzFInVNdQBN8fW1I4JQT5JiTj3TBCWZOlDbV7Pttvn2cEKu",
};
const MANIFEST: &str = "https://github.com/kagan-sh/kagan/releases/latest/download/update.json";
const DAY: Duration = Duration::from_hours(24);
const TICK: Duration = Duration::from_hours(1);
const LIMIT: u64 = 512 * 1024 * 1024;

/// `update.json`, read only after its own signature checks.
#[derive(Deserialize)]
struct Manifest {
    version: String,
    platforms: HashMap<String, Asset>,
}

#[derive(Clone, Deserialize)]
struct Asset {
    url: String,
    signature: String,
}

#[derive(Debug, thiserror::Error)]
enum UpdateError {
    #[error("bad public key: {0}")]
    Key(String),
    #[error("the update has no readable signature")]
    Signature,
    #[error("the update's signature does not match kagan's key")]
    Mismatch,
    #[error("{0}")]
    Client(String),
    #[error("bad link {url}: {reason}")]
    Link { url: String, reason: String },
    #[error("kagan updates only over HTTPS, not {0}")]
    Insecure(String),
    #[error("could not reach {url}: {reason}")]
    Reach { url: String, reason: String },
    #[error("{url} is larger than {} MiB", LIMIT / 1024 / 1024)]
    TooBig { url: String },
    #[error("could not download {url}: {reason}")]
    Download { url: String, reason: String },
    #[error("the update list's signature does not match kagan's key")]
    ListSignature,
    #[error("the update list is not valid: {0}")]
    List(String),
    #[error("{0}")]
    Step(String),
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
    restart: bool,
    checked: Option<Instant>,
    _automatic: Task<()>,
    _quit: Subscription,
}

impl Updater {
    pub(crate) fn install(database: Database, cx: &mut App) {
        let updater = cx.new(|cx: &mut Context<Self>| {
            let target = Target::detect();
            let cleaning = target.clone();
            cx.background_spawn(async move { cleaning.clean() })
                .detach();
            Self {
                version: current(),
                target,
                endpoint: std::env::var("KG_UPDATE_URL").unwrap_or_else(|_| MANIFEST.into()),
                key: KEY.into(),
                state: UpdateState::Unchecked,
                asset: None,
                staged: None,
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
    pub fn kind(&self) -> InstallKind {
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
        let key = self.key.clone();
        let fetch = cx.background_spawn(async move { manifest(&endpoint, &key) });
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
                                notes: notes(&version),
                                version: version.to_string(),
                            }
                        }
                        None => UpdateState::Current,
                    };
                    this.set(state, cx);
                    Ok(())
                }
                Err(error) => {
                    let reason = error.to_string();
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

    /// Downloads the offered version, checks its signature and puts it beside this copy. A
    /// download already done only changes whether it installs at quit.
    /// Not an `Op`: only the person installs, from the window.
    pub fn download(&mut self, at_quit: bool, cx: &mut Context<Self>) -> Task<Result<()>> {
        if let UpdateState::Ready { version, notes, .. } = self.state.clone() {
            self.set(
                UpdateState::Ready {
                    version,
                    notes,
                    at_quit,
                },
                cx,
            );
            return Task::ready(Ok(()));
        }
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
            target
                .prepare()
                .map_err(|error| UpdateError::Step(error.to_string()))?;
            let bytes = get(&asset.url)?;
            verify(&bytes, &asset.signature, &key)?;
            target
                .stage(&bytes)
                .map_err(|error| UpdateError::Step(error.to_string()))
        });
        cx.spawn(async move |this, cx| {
            let staged = work.await;
            this.update(cx, |this, cx| match staged {
                Ok(path) => {
                    this.staged = Some(path);
                    this.set(
                        UpdateState::Ready {
                            version,
                            notes,
                            at_quit,
                        },
                        cx,
                    );
                    Ok(())
                }
                Err(error) => {
                    let reason = error.to_string();
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
            if target.install == InstallKind::Installer {
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
        let at_quit = matches!(self.state, UpdateState::Ready { at_quit: true, .. });
        let staged = self.staged.take().filter(|_| at_quit);
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

    #[cfg(any(test, feature = "test"))]
    pub fn use_state(&mut self, state: UpdateState, target: Target, cx: &mut Context<Self>) {
        self.target = target;
        self.set(state, cx);
    }

    #[cfg(any(test, feature = "test"))]
    pub fn use_source(&mut self, endpoint: String, key: String, target: Target) {
        self.endpoint = endpoint;
        self.key = key;
        self.target = target;
    }
}

pub(crate) async fn run(database: &Database, op: UpdateOp, cx: &mut AsyncApp) -> Result<OpResult> {
    let updater = cx.update(|cx| Updater::global(cx));
    if op == UpdateOp::Check {
        updater.update(cx, Updater::check).await?;
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

fn notes(version: &Version) -> String {
    format!("https://github.com/kagan-sh/kagan/releases/tag/v{version}")
}

pub(crate) fn newer(current: &Version, offered: &str) -> Option<Version> {
    let offered = Version::parse(offered.trim_start_matches('v')).ok()?;
    (offered.pre.is_empty() && offered > *current).then_some(offered)
}

/// `signature` is the `.sig` file cargo-packager writes: a minisign signature, base64 encoded.
fn verify(bytes: &[u8], signature: &str, key: &str) -> std::result::Result<(), UpdateError> {
    let key = PublicKey::from_base64(key).map_err(|error| UpdateError::Key(error.to_string()))?;
    let text = base64::engine::general_purpose::STANDARD
        .decode(signature.trim())
        .ok()
        .and_then(|text| String::from_utf8(text).ok())
        .ok_or(UpdateError::Signature)?;
    let signature = Signature::decode(&text).map_err(|_| UpdateError::Signature)?;
    key.verify(bytes, &signature, false)
        .map_err(|_| UpdateError::Mismatch)
}

/// HTTPS, or plain HTTP to this machine for tests and local proof. Everything read is signed.
fn allowed(url: &reqwest::Url) -> bool {
    url.scheme() == "https"
        || (url.scheme() == "http"
            && matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]")))
}

fn client() -> std::result::Result<reqwest::blocking::Client, UpdateError> {
    reqwest::blocking::Client::builder()
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() > 10 {
                attempt.error("too many redirects")
            } else if allowed(attempt.url()) {
                attempt.follow()
            } else {
                attempt.error("an update link left HTTPS")
            }
        }))
        .user_agent(concat!("kagan/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(15))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|error| UpdateError::Client(error.to_string()))
}

fn get(url: &str) -> std::result::Result<Vec<u8>, UpdateError> {
    let parsed = reqwest::Url::parse(url).map_err(|error| UpdateError::Link {
        url: url.to_owned(),
        reason: error.to_string(),
    })?;
    if !allowed(&parsed) {
        return Err(UpdateError::Insecure(url.to_owned()));
    }
    let response = client()?
        .get(parsed)
        .send()
        .and_then(reqwest::blocking::Response::error_for_status)
        .map_err(|error| UpdateError::Reach {
            url: url.to_owned(),
            reason: error.to_string(),
        })?;
    let too_big = || UpdateError::TooBig {
        url: url.to_owned(),
    };
    if response
        .content_length()
        .is_some_and(|length| length > LIMIT)
    {
        return Err(too_big());
    }
    let mut bytes = Vec::new();
    response
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| UpdateError::Download {
            url: url.to_owned(),
            reason: error.to_string(),
        })?;
    if bytes.len() as u64 > LIMIT {
        return Err(too_big());
    }
    Ok(bytes)
}

fn manifest(url: &str, key: &str) -> std::result::Result<Manifest, UpdateError> {
    let bytes = get(url)?;
    let signature = get(&format!("{url}.sig"))?;
    verify(&bytes, &String::from_utf8_lossy(&signature), key)
        .map_err(|_| UpdateError::ListSignature)?;
    serde_json::from_slice(&bytes).map_err(|error| UpdateError::List(error.to_string()))
}
