//! A plugin's secret settings: where they are kept, and the one place that knows their text, so
//! everything leaving a plugin can be stripped of it.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Write as _;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};

use keyring_core::{CredentialStore, Entry};
use kg_domain::{OpError, PluginId};
use serde::{Deserialize, Serialize};

const SERVICE: &str = "kagan";
const HIDDEN: &str = "[secret]";
/// Windows Credential Manager keeps at most 2560 bytes of UTF-16, so every system takes
/// the same 1024 bytes.
pub(crate) const MAX_BYTES: usize = 1024;
/// The account names the key, and Windows Credential Manager caps it at 513 characters.
pub(crate) const MAX_KEY: usize = 64;

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("the system keychain refused: {0}")]
    Keychain(#[source] keyring_core::Error),
    #[error("the secrets file: {0}")]
    File(#[from] std::io::Error),
    #[error("the secrets file: {0}")]
    Format(#[from] serde_json::Error),
    #[error("the secret store stopped after a panic")]
    Poisoned,
    #[error("the system keychain holds this secret and cannot be reached")]
    Unreachable,
}

impl From<Error> for OpError {
    fn from(error: Error) -> Self {
        OpError::Failed(error.to_string())
    }
}

#[derive(Clone)]
pub(crate) struct Store {
    keychain: Option<Arc<CredentialStore>>,
    data_dir: PathBuf,
    /// Held across every read and write of a keychain item or a file, never by the UI thread.
    io: Arc<Mutex<()>>,
    known: Arc<Mutex<Known>>,
}

#[derive(Default)]
struct Known {
    texts: BTreeMap<PluginId, BTreeMap<String, String>>,
    /// The keychain was asked and is missing, so secrets go to the file.
    no_keychain: bool,
}

/// `<data>/secrets/<plugin id>.json`: the keys the keychain holds, by name only, and the
/// text of any key kept in the file instead.
#[derive(Default, Serialize, Deserialize)]
struct Kept {
    #[serde(default)]
    keychain: BTreeSet<String>,
    #[serde(default)]
    file: BTreeMap<String, String>,
}

impl Kept {
    fn keys(&self) -> impl Iterator<Item = &String> {
        self.keychain.iter().chain(self.file.keys())
    }

    /// The keychain item, or `None` when the key goes to the file. A key the keychain holds
    /// is never moved to the file or forgotten while the keychain cannot be reached.
    fn entry(&self, store: &Store, id: &PluginId, key: &str) -> Result<Option<Entry>, Error> {
        match store.entry(id, key)? {
            None if self.keychain.contains(key) => Err(Error::Unreachable),
            entry => Ok(entry),
        }
    }
}

impl Store {
    pub(crate) fn new(keychain: Option<Arc<CredentialStore>>, data_dir: PathBuf) -> Self {
        Self {
            keychain,
            data_dir,
            io: Arc::default(),
            known: Arc::default(),
        }
    }

    /// The warning a person sees once secrets are known to go to the file.
    pub(crate) fn warning(&self) -> Option<String> {
        let missing = self.keychain.is_none() || self.known().is_ok_and(|known| known.no_keychain);
        missing.then(|| {
            format!(
                "This system has no keychain kagan can use, so secrets are kept in {}, \
                 in files only you can read. Programs you run can still read them.",
                self.folder().display()
            )
        })
    }

    pub(crate) fn is_set(&self, id: &PluginId, key: &str) -> bool {
        self.known().is_ok_and(|known| {
            known
                .texts
                .get(id)
                .is_some_and(|texts| texts.contains_key(key))
        })
    }

    pub(crate) fn texts(&self, id: &PluginId) -> BTreeMap<String, String> {
        self.known()
            .ok()
            .and_then(|known| known.texts.get(id).cloned())
            .unwrap_or_default()
    }

    /// `text` with each of the plugin's secrets replaced, the longest first so one that starts
    /// another cannot leave the rest of it behind. Only the exact text is found.
    pub(crate) fn redact(&self, id: &PluginId, text: &str) -> String {
        let Ok(known) = self.known() else {
            return HIDDEN.to_owned();
        };
        let mut secrets: Vec<&str> = known
            .texts
            .get(id)
            .map(|texts| texts.values().map(String::as_str).collect())
            .unwrap_or_default();
        secrets.sort_by_key(|secret| std::cmp::Reverse(secret.len()));
        secrets
            .into_iter()
            .fold(text.to_owned(), |text, secret| text.replace(secret, HIDDEN))
    }

    /// The only way kagan writes a plugin's log, so no line holds a secret.
    pub(crate) fn log(
        &self,
        id: &PluginId,
        level: crate::Level,
        message: &str,
    ) -> crate::Result<()> {
        crate::log(&self.data_dir, id, level, &self.redact(id, message))
    }

    /// Reads the declared secrets into memory and forgets any kept key the manifest no longer
    /// declares. Returns what could not be read or forgotten.
    pub(crate) fn load(&self, id: &PluginId, declared: &[String]) -> Vec<String> {
        let Ok(_io) = self.io() else {
            return vec![Error::Poisoned.to_string()];
        };
        let mut kept = match self.read_file(id) {
            Ok(kept) => kept,
            Err(error) => return vec![error.to_string()],
        };
        let mut problems = Vec::new();
        let stale: Vec<String> = kept
            .keys()
            .filter(|key| !declared.contains(key))
            .cloned()
            .collect();
        for key in stale {
            if let Err(error) = self.drop_key(&mut kept, id, &key) {
                problems.push(format!(
                    "secret setting {key} could not be forgotten: {error}"
                ));
            }
        }
        let mut texts = BTreeMap::new();
        for key in declared {
            match self.read_key(&kept, id, key) {
                Ok(Some(text)) => {
                    texts.insert(key.clone(), text);
                }
                Ok(None) => {}
                Err(error) => {
                    problems.push(format!("secret setting {key} could not be read: {error}"));
                }
            }
        }
        if let Err(error) = self.write_file(id, &kept) {
            problems.push(error.to_string());
        }
        if let Ok(mut known) = self.known() {
            known.texts.insert(id.clone(), texts);
        }
        problems
    }

    /// Keeps `text`, and returns the warning when it went to the file.
    pub(crate) fn set(
        &self,
        id: &PluginId,
        key: &str,
        text: &str,
    ) -> Result<Option<String>, Error> {
        let _io = self.io()?;
        let mut kept = self.read_file(id)?;
        if let Some(entry) = kept.entry(self, id, key)? {
            entry.set_password(text).map_err(Error::Keychain)?;
            kept.keychain.insert(key.to_owned());
            kept.file.remove(key);
        } else {
            kept.file.insert(key.to_owned(), text.to_owned());
        }
        self.write_file(id, &kept)?;
        self.known()?
            .texts
            .entry(id.clone())
            .or_default()
            .insert(key.to_owned(), text.to_owned());
        Ok(self.warning().filter(|_| kept.file.contains_key(key)))
    }

    pub(crate) fn clear(&self, id: &PluginId, key: &str) -> Result<(), Error> {
        let _io = self.io()?;
        let mut kept = self.read_file(id)?;
        self.drop_key(&mut kept, id, key)?;
        self.write_file(id, &kept)
    }

    /// Forgets every secret the plugin kept or declares. Returns the keys the keychain would
    /// not let go of; their names stay in the file.
    pub(crate) fn forget(&self, id: &PluginId, declared: &[String]) -> Result<Vec<String>, Error> {
        let _io = self.io()?;
        let mut kept = self.read_file(id)?;
        let mut keys: BTreeSet<String> = kept.keys().cloned().collect();
        keys.extend(declared.iter().cloned());
        let failed = keys
            .into_iter()
            .filter(|key| self.drop_key(&mut kept, id, key).is_err())
            .collect();
        self.write_file(id, &kept)?;
        self.known()?.texts.remove(id);
        Ok(failed)
    }

    fn read_key(&self, kept: &Kept, id: &PluginId, key: &str) -> Result<Option<String>, Error> {
        if let Some(entry) = kept.entry(self, id, key)? {
            match entry.get_password() {
                Ok(text) => return Ok(Some(text)),
                Err(keyring_core::Error::NoEntry) => {}
                Err(error) => return Err(Error::Keychain(error)),
            }
        }
        Ok(kept.file.get(key).cloned())
    }

    fn drop_key(&self, kept: &mut Kept, id: &PluginId, key: &str) -> Result<(), Error> {
        if let Some(entry) = kept.entry(self, id, key)? {
            match entry.delete_credential() {
                Ok(()) | Err(keyring_core::Error::NoEntry) => {}
                Err(error) => return Err(Error::Keychain(error)),
            }
        }
        kept.keychain.remove(key);
        kept.file.remove(key);
        if let Some(texts) = self.known()?.texts.get_mut(id) {
            texts.remove(key);
        }
        Ok(())
    }

    fn entry(&self, id: &PluginId, key: &str) -> Result<Option<Entry>, Error> {
        let Some(keychain) = &self.keychain else {
            return Ok(None);
        };
        if self.known()?.no_keychain {
            return Ok(None);
        }
        // The data directory keeps a second kagan, such as one under KG_DATA_DIR, apart.
        let account = format!("{id}/{key} in {}", self.data_dir.display());
        match keychain.build(SERVICE, &account, None) {
            Ok(entry) => Ok(Some(entry)),
            Err(keyring_core::Error::NoDefaultStore) => {
                self.known()?.no_keychain = true;
                Ok(None)
            }
            Err(error) => Err(Error::Keychain(error)),
        }
    }

    fn io(&self) -> Result<MutexGuard<'_, ()>, Error> {
        self.io.lock().map_err(|_| Error::Poisoned)
    }

    fn known(&self) -> Result<MutexGuard<'_, Known>, Error> {
        self.known.lock().map_err(|_| Error::Poisoned)
    }

    fn folder(&self) -> PathBuf {
        self.data_dir.join("secrets")
    }

    fn file(&self, id: &PluginId) -> PathBuf {
        self.folder().join(format!("{id}.json"))
    }

    fn read_file(&self, id: &PluginId) -> Result<Kept, Error> {
        match std::fs::read(self.file(id)) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Kept::default()),
            Err(error) => Err(error.into()),
        }
    }

    fn write_file(&self, id: &PluginId, kept: &Kept) -> Result<(), Error> {
        let file = self.file(id);
        if kept.keychain.is_empty() && kept.file.is_empty() {
            return match std::fs::remove_file(&file) {
                Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
                _ => Ok(()),
            };
        }
        let folder = self.folder();
        std::fs::create_dir_all(&folder)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&folder, std::fs::Permissions::from_mode(0o700))?;
        }
        // A new temporary file is readable only by its owner before any byte is written.
        let mut temporary = tempfile::NamedTempFile::new_in(&folder)?;
        temporary.write_all(&serde_json::to_vec(kept)?)?;
        temporary.as_file().sync_all()?;
        temporary.persist(&file).map_err(|error| error.error)?;
        Ok(())
    }
}
