//! The system keychain that plugin secrets go to.

/// Plugin secrets go to the system keychain, opened on first use: a Linux D-Bus can take
/// long to answer, and nothing should wait on it before a secret is needed.
pub(crate) fn use_keychain() {
    keyring_core::set_default_store(std::sync::Arc::new(Keychain(std::sync::OnceLock::new())));
}

/// The system keychain, opened by the first secret read or write and kept for the whole run.
/// Without one, it answers `NoDefaultStore`, and the plugin host keeps secrets in a file.
struct Keychain(std::sync::OnceLock<Option<std::sync::Arc<keyring_core::CredentialStore>>>);

impl keyring_core::api::CredentialStoreApi for Keychain {
    fn vendor(&self) -> String {
        "kagan".to_owned()
    }

    fn id(&self) -> String {
        "kagan".to_owned()
    }

    fn build(
        &self,
        service: &str,
        user: &str,
        modifiers: Option<&std::collections::HashMap<&str, &str>>,
    ) -> keyring_core::Result<keyring_core::Entry> {
        self.0
            .get_or_init(open_keychain)
            .as_ref()
            .ok_or(keyring_core::Error::NoDefaultStore)?
            .build(service, user, modifiers)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn open_keychain() -> Option<std::sync::Arc<keyring_core::CredentialStore>> {
    #[cfg(target_os = "macos")]
    let store = apple_native_keyring_store::keychain::Store::new();
    #[cfg(windows)]
    let store = windows_native_keyring_store::Store::new();
    #[cfg(all(unix, not(target_os = "macos")))]
    let store = zbus_secret_service_keyring_store::Store::new();
    store
        .ok()
        .map(|store| store as std::sync::Arc<keyring_core::CredentialStore>)
}
