//! The `WebKit` data stores that plugin pages keep, and their deletion.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use gpui_kit::AsyncApp;
use kg_domain::{OpError, PluginId};

#[must_use]
pub(crate) fn store_dir(data: &Path, page: &str) -> PathBuf {
    data.join("pages").join(store_name(page))
}

/// The page store's 16-byte id, written to `store.id` in [`store_dir`] on first use.
///
/// # Errors
///
/// The folder cannot be created, or the id file cannot be written.
pub fn keep_store(data: &Path, page: &str) -> Result<[u8; 16], OpError> {
    let dir = store_dir(data, page);
    std::fs::create_dir_all(&dir)
        .map_err(|error| OpError::Failed(format!("{}: {error}", dir.display())))?;
    let path = dir.join("store.id");
    if let Some(id) = read_id(&path) {
        return Ok(id);
    }
    let id = *uuid::Uuid::new_v4().as_bytes();
    std::fs::write(&path, id)
        .map_err(|error| OpError::Failed(format!("{}: {error}", path.display())))?;
    Ok(id)
}

/// Skips folders without a valid 16-byte `store.id`.
#[must_use]
pub(crate) fn read_store_ids(data: &Path) -> Vec<[u8; 16]> {
    let pages = data.join("pages");
    let Ok(entries) = std::fs::read_dir(&pages) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        ids.extend(read_id(&entry.path().join("store.id")));
    }
    ids
}

fn read_id(path: &Path) -> Option<[u8; 16]> {
    std::fs::read(path).ok()?.try_into().ok()
}

/// Deletes `pages/<page>/` only when its `store.id` is in `removed`. Other folders stay, so a later
/// start can ask `WebKit` again.
pub(crate) fn clear_removed_stores(data: &Path, removed: &[[u8; 16]]) {
    if removed.is_empty() {
        return;
    }
    let pages = data.join("pages");
    let Ok(entries) = std::fs::read_dir(&pages) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if read_id(&path.join("store.id")).is_some_and(|id| removed.contains(&id)) {
            // A folder that stays keeps its `store.id`, so the next start tries again.
            let _ = std::fs::remove_dir_all(&path);
        }
    }
    if std::fs::read_dir(&pages).is_ok_and(|mut entries| entries.next().is_none()) {
        // Only an empty folder goes; one that is not empty is left as it is.
        let _ = std::fs::remove_dir(&pages);
    }
}

pub(crate) struct StoreRemoval {
    pub(crate) removed: Vec<[u8; 16]>,
    pub(crate) warnings: Vec<String>,
}

// Wait before a store delete so GPUI drops the native `WebView` and its frame mouse listener.
pub(crate) const RELEASE_WAIT: std::time::Duration = std::time::Duration::from_millis(50);
// Time allowed for retries after `InUse`. The first release wait does not count.
const RETRY_BUDGET: std::time::Duration = std::time::Duration::from_secs(3);
const RETRY_START: std::time::Duration = std::time::Duration::from_millis(50);
const RETRY_MAX: std::time::Duration = std::time::Duration::from_millis(250);

// `InUse` means `WebKit` still holds the store; a retry may work.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StoreAttempt {
    Removed,
    #[cfg(any(target_os = "macos", test))]
    InUse,
    #[cfg(any(target_os = "macos", test))]
    Failed(String),
}

#[cfg(test)]
type RemoveHook = Box<dyn FnMut([u8; 16]) -> StoreAttempt>;

#[cfg(test)]
thread_local! {
    static REMOVE_HOOK: std::cell::RefCell<Option<RemoveHook>> = const { std::cell::RefCell::new(None) };
}

// Replaces `remove_data_store` in tests until the guard drops.
#[cfg(test)]
pub(crate) fn set_remove_hook(
    hook: impl FnMut([u8; 16]) -> StoreAttempt + 'static,
) -> RemoveHookGuard {
    REMOVE_HOOK.with(|cell| {
        *cell.borrow_mut() = Some(Box::new(hook));
    });
    RemoveHookGuard
}

#[cfg(test)]
pub(crate) struct RemoveHookGuard;

#[cfg(test)]
impl Drop for RemoveHookGuard {
    fn drop(&mut self) {
        REMOVE_HOOK.with(|cell| {
            *cell.borrow_mut() = None;
        });
    }
}

// Retries `DataStoreInUse` with growing waits, for about 3 seconds after the first release wait.
// Ids still held stay out of `removed`, so their `store.id` files
// stay for a later start.
pub(crate) async fn remove_web_stores_after_release(
    ids: Vec<[u8; 16]>,
    cx: &mut AsyncApp,
) -> StoreRemoval {
    if let Some(done) = no_stores(&ids) {
        return done;
    }
    cx.background_executor().timer(RELEASE_WAIT).await;
    let mut pending = ids;
    let mut removed = Vec::new();
    let mut warnings = Vec::new();
    let mut delay = RETRY_START;
    let mut waited = RELEASE_WAIT;
    loop {
        let mut attempts = Vec::new();
        for id in pending {
            attempts.push((id, remove_one_store(id, cx).await));
        }
        let (done, still, failed) = tally(attempts);
        removed.extend(done);
        warnings.extend(failed);
        if still.is_empty() || waited >= RETRY_BUDGET + RELEASE_WAIT {
            for _ in &still {
                warnings.push(IN_USE_WARNING.into());
            }
            return StoreRemoval { removed, warnings };
        }
        cx.background_executor().timer(delay).await;
        waited = waited.saturating_add(delay);
        delay = (delay.saturating_mul(2)).min(RETRY_MAX);
        pending = still;
    }
}

async fn remove_one_store(id: [u8; 16], cx: &mut AsyncApp) -> StoreAttempt {
    #[cfg(test)]
    if let Some(attempt) = REMOVE_HOOK.with(|cell| cell.borrow_mut().as_mut().map(|hook| hook(id)))
    {
        return attempt;
    }
    remove_one_native(id, cx).await
}

#[cfg(target_os = "macos")]
async fn remove_one_native(id: [u8; 16], cx: &mut AsyncApp) -> StoreAttempt {
    use wry::WebViewExtDarwin as _;

    let (send, receive) = futures::channel::oneshot::channel();
    cx.update(|_| {
        wry::WebView::remove_data_store(&id, move |result| {
            // The task waiting for the result may have been dropped.
            let _ = send.send(result);
        });
    });
    match receive.await {
        Ok(Err(wry::Error::DataStoreInUse)) => StoreAttempt::InUse,
        Ok(Err(error)) => StoreAttempt::Failed(format!(
            "the plugin is removed, but its page store was not deleted: {error}"
        )),
        Ok(Ok(())) | Err(_) => StoreAttempt::Removed,
    }
}

#[cfg(not(target_os = "macos"))]
fn remove_one_native(
    _id: [u8; 16],
    _cx: &mut AsyncApp,
) -> impl std::future::Future<Output = StoreAttempt> {
    std::future::ready(StoreAttempt::Removed)
}

// Deletes each store once, with no wait or retry.
pub(crate) async fn remove_web_stores(ids: Vec<[u8; 16]>, cx: &mut AsyncApp) -> StoreRemoval {
    if let Some(done) = no_stores(&ids) {
        return done;
    }
    let mut attempts = Vec::new();
    for id in ids {
        attempts.push((id, remove_one_store(id, cx).await));
    }
    let (removed, still, mut warnings) = tally(attempts);
    for _ in &still {
        warnings.push(IN_USE_WARNING.into());
    }
    StoreRemoval { removed, warnings }
}

const IN_USE_WARNING: &str =
    "the plugin is removed, but WebKit still held a page store (DataStoreInUse)";

/// The answer for an empty list or an old macOS, where custom stores do not exist.
fn no_stores(ids: &[[u8; 16]]) -> Option<StoreRemoval> {
    if ids.is_empty() {
        return Some(StoreRemoval {
            removed: Vec::new(),
            warnings: Vec::new(),
        });
    }
    #[cfg(target_os = "macos")]
    if macos_major() < 14 {
        return Some(StoreRemoval {
            removed: ids.to_vec(),
            warnings: Vec::new(),
        });
    }
    None
}

/// Splits attempts into deleted ids, ids `WebKit` still holds, and failures.
fn tally(attempts: Vec<([u8; 16], StoreAttempt)>) -> (Vec<[u8; 16]>, Vec<[u8; 16]>, Vec<String>) {
    let mut removed = Vec::new();
    #[cfg_attr(
        not(any(target_os = "macos", test)),
        expect(unused_mut, reason = "the InUse and Failed arms are macOS-only")
    )]
    let mut still = Vec::new();
    #[cfg_attr(
        not(any(target_os = "macos", test)),
        expect(unused_mut, reason = "the InUse and Failed arms are macOS-only")
    )]
    let mut warnings = Vec::new();
    for (id, attempt) in attempts {
        match attempt {
            StoreAttempt::Removed => removed.push(id),
            #[cfg(any(target_os = "macos", test))]
            StoreAttempt::InUse => still.push(id),
            #[cfg(any(target_os = "macos", test))]
            StoreAttempt::Failed(message) => warnings.push(message),
        }
    }
    (removed, still, warnings)
}

// Retries `WebKit` store deletion for data folders of plugins that are no longer installed. An
// earlier remove can leave `store.id` behind while `WebKit` still holds the view.
pub(crate) async fn retry_orphan_stores(
    data_dir: &Path,
    known: &BTreeSet<PluginId>,
    cx: &mut AsyncApp,
) {
    let Ok(entries) = std::fs::read_dir(kg_core::plugins_dir(data_dir)) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let id = PluginId(name.into());
        if known.contains(&id) {
            continue;
        }
        let data = kg_core::plugin_data_dir(data_dir, &id);
        let ids = read_store_ids(&data);
        if ids.is_empty() {
            continue;
        }
        let cleared = remove_web_stores(ids, cx).await;
        clear_removed_stores(&data, &cleared.removed);
    }
}

#[cfg(target_os = "macos")]
#[must_use]
pub fn macos_major() -> u32 {
    use objc2_foundation::NSProcessInfo;
    let version = NSProcessInfo::processInfo().operatingSystemVersion();
    u32::try_from(version.majorVersion).unwrap_or(0)
}

fn store_name(page: &str) -> String {
    let safe = !matches!(page, "" | "." | "..")
        && page.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-' || byte == b'_'
        });
    if safe {
        page.to_owned()
    } else {
        let mut name = String::with_capacity(page.len() * 2);
        for byte in page.as_bytes() {
            use std::fmt::Write as _;
            // Writing to a String cannot fail.
            let _ = write!(name, "{byte:02x}");
        }
        name
    }
}

#[cfg(test)]
mod store_tests {
    use super::*;

    #[test]
    fn keep_store_reuses_the_same_id() {
        let data = tempfile::tempdir().expect("temp");
        let first = keep_store(data.path(), "board").expect("first");
        let second = keep_store(data.path(), "board").expect("second");
        assert_eq!(first, second);
        assert!(store_dir(data.path(), "board").join("store.id").is_file());
    }

    #[test]
    fn store_name_hexes_unsafe_ids() {
        assert_eq!(store_name("board"), "board");
        assert_eq!(store_name("a/b"), "612f62");
        assert_eq!(store_name("."), "2e");
        assert_eq!(store_name(".."), "2e2e");
        assert_ne!(store_name(".."), store_name("."));
    }

    #[test]
    fn read_store_ids_lists_each_page() {
        let data = tempfile::tempdir().expect("temp");
        let first = keep_store(data.path(), "board").expect("first");
        let second = keep_store(data.path(), "other").expect("second");
        let mut ids = read_store_ids(data.path());
        ids.sort_unstable();
        let mut want = vec![first, second];
        want.sort_unstable();
        assert_eq!(ids, want);
    }

    #[test]
    fn clear_removed_stores_keeps_ids_web_kit_still_held() {
        let data = tempfile::tempdir().expect("temp");
        let gone = keep_store(data.path(), "board").expect("gone");
        let kept = keep_store(data.path(), "other").expect("kept");
        clear_removed_stores(data.path(), &[gone]);
        assert!(!store_dir(data.path(), "board").exists());
        assert!(store_dir(data.path(), "other").join("store.id").is_file());
        assert_eq!(read_store_ids(data.path()), vec![kept]);
    }
}
