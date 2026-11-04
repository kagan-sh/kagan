//! A panel's own HTML page: the files it may read, and the JSON bridge to its script.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};
use std::sync::Weak;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use gpui_shell::{HostError, HostModule, HostValue};
use kg_domain::PluginId;
use serde_json::{Map, Value};

use crate::area::from_host;
use crate::jobs::Jobs;
use crate::ui_host::{self, HostUiEvent};

/// The largest JSON text `kagan.post` or `post` accepts, in bytes.
pub const MESSAGE_CAP: usize = 1024 * 1024;
// The largest file the page may read from the plugin folder.
pub(crate) const FILE_CAP: u64 = 8 * 1024 * 1024;

/// The custom-scheme name. The page origin is `kagan-plugin://<plugin-id>`.
pub const SCHEME: &str = "kagan-plugin";

const RESERVED: &str = "__kagan";

#[derive(Debug, PartialEq, Eq)]
pub enum PageFileError {
    Missing,
    Refused,
    TooBig,
}

impl std::fmt::Display for PageFileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => f.write_str("the page file is not in the plugin folder"),
            Self::Refused => f.write_str("the page path leaves the plugin folder"),
            Self::TooBig => f.write_str("the page file is over the 8 MiB limit"),
        }
    }
}

impl std::error::Error for PageFileError {}

/// Reads one file inside `root`. `..`, an absolute path, and any symlink are refused.
///
/// # Errors
///
/// The path leaves the folder, the file is missing, or the file is over the size cap.
pub fn read_plugin_file(root: &Path, relative: &str) -> Result<Vec<u8>, PageFileError> {
    let relative = relative_file(relative)?;
    let meta = std::fs::symlink_metadata(root).map_err(|_| PageFileError::Refused)?;
    if meta.file_type().is_symlink() || !meta.is_dir() {
        return Err(PageFileError::Refused);
    }
    let dir =
        Dir::open_ambient_dir(root, ambient_authority()).map_err(|_| PageFileError::Refused)?;
    read_in(&dir, &relative)
}

// Checks the declared `web` file when the manifest is read.
pub(crate) fn require_html(root: &Path, relative: &str) -> Result<(), String> {
    shape(relative)?;
    read_plugin_file(root, relative)
        .map(|_| ())
        .map_err(|error| format!("web {relative:?} cannot be the panel page: {error}"))
}

/// `raw` is one JSON value, at most [`MESSAGE_CAP`] bytes.
///
/// # Errors
///
/// The text is over that cap, or it is not JSON.
pub fn parse_message(raw: &str) -> Result<Value, String> {
    if raw.len() > MESSAGE_CAP {
        return Err(format!(
            "a page message is {} bytes, over the {MESSAGE_CAP}-byte limit",
            raw.len()
        ));
    }
    serde_json::from_str(raw).map_err(|_| "a page message must be JSON".to_owned())
}

pub(crate) fn module(plugin: &PluginId) -> HostModule {
    let owner = plugin.clone();
    HostModule::new("kagan/panel")
        .declarations(
            "/** Sends one JSON value to this panel's page. At most 1 MiB. The page hears it as a `kagan` event. */\n\
             export function post(panelId: string, message: unknown): Promise<void>;",
        )
        .async_function("post", move |arguments| {
            let panel = arguments.string(0)?.to_owned();
            let message = from_host(arguments.get(1).unwrap_or(&HostValue::Null));
            let text = serde_json::to_string(&message)
                .map_err(|_| HostError::new("a page message must be JSON"))?;
            parse_message(&text).map_err(HostError::new)?;
            let (reply, replied) = async_channel::bounded(1);
            let plugin = owner.clone();
            ui_host::app(|ui, cx| {
                ui.update(cx, |_, cx| {
                    cx.emit(HostUiEvent::PostPage {
                        plugin,
                        panel,
                        message: text,
                        reply,
                    });
                });
                Ok(())
            })?;
            Ok(ui_host::accepted(replied))
        })
}

pub(crate) fn enqueue(
    jobs: &Jobs,
    panel: &str,
    message: Value,
    alive: Weak<()>,
    bytes: usize,
) -> Option<(String, async_channel::Receiver<Result<Value, String>>)> {
    let mut job = Map::new();
    job.insert("kind".into(), "page".into());
    job.insert("panelId".into(), panel.into());
    job.insert("message".into(), message);
    jobs.send_page(job, alive, bytes)
}

fn shape(relative: &str) -> Result<(), String> {
    if !is_html(relative)
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains('\0')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part.eq_ignore_ascii_case(RESERVED))
    {
        return Err(format!(
            "web {relative:?} must be an .html file inside the plugin"
        ));
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(format!("web {relative:?} leaves the plugin folder"));
    }
    Ok(())
}

fn relative_file(relative: &str) -> Result<PathBuf, PageFileError> {
    if relative.is_empty()
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains('\0')
        || relative
            .split('/')
            .any(|part| part.is_empty() || part.eq_ignore_ascii_case(RESERVED))
    {
        return Err(PageFileError::Refused);
    }
    let path = Path::new(relative);
    if path.is_absolute()
        || !path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
    {
        return Err(PageFileError::Refused);
    }
    Ok(path.to_path_buf())
}

fn read_in(dir: &Dir, relative: &Path) -> Result<Vec<u8>, PageFileError> {
    let names: Vec<_> = relative.components().collect();
    let mut walked = PathBuf::new();
    for (index, component) in names.iter().enumerate() {
        let Component::Normal(name) = component else {
            return Err(PageFileError::Refused);
        };
        walked.push(name);
        let meta = dir
            .symlink_metadata(&walked)
            .map_err(|error| classify(&error))?;
        if meta.file_type().is_symlink() {
            return Err(PageFileError::Refused);
        }
        let last = index + 1 == names.len();
        if last {
            if !meta.is_file() || meta.len() > FILE_CAP {
                return Err(if meta.len() > FILE_CAP {
                    PageFileError::TooBig
                } else {
                    PageFileError::Refused
                });
            }
        } else if !meta.is_dir() {
            return Err(PageFileError::Refused);
        }
    }
    let file = dir.open(relative).map_err(|_| PageFileError::Refused)?;
    let mut bytes = Vec::new();
    file.take(FILE_CAP + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| PageFileError::Refused)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > FILE_CAP {
        return Err(PageFileError::TooBig);
    }
    Ok(bytes)
}

fn is_html(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("html"))
}

fn classify(error: &std::io::Error) -> PageFileError {
    if error.kind() == std::io::ErrorKind::NotFound {
        PageFileError::Missing
    } else {
        PageFileError::Refused
    }
}

/// Folder for one page's durable store files under the plugin data directory.
#[must_use]
pub fn store_dir(data: &Path, page: &str) -> PathBuf {
    data.join("pages").join(store_name(page))
}

/// Stable 16-byte id for a page store, kept under [`store_dir`].
///
/// # Errors
///
/// The folder cannot be created or the id file cannot be read or written.
pub fn keep_store(data: &Path, page: &str) -> Result<[u8; 16], String> {
    let dir = store_dir(data, page);
    std::fs::create_dir_all(&dir).map_err(|error| error.to_string())?;
    let path = dir.join("store.id");
    if let Ok(bytes) = std::fs::read(&path)
        && bytes.len() == 16
    {
        let mut id = [0u8; 16];
        id.copy_from_slice(&bytes);
        return Ok(id);
    }
    let id = *uuid::Uuid::new_v4().as_bytes();
    std::fs::write(&path, id).map_err(|error| error.to_string())?;
    Ok(id)
}

/// Reads every durable store id under this plugin's data directory.
#[must_use]
pub fn read_store_ids(data: &Path) -> Vec<[u8; 16]> {
    let pages = data.join("pages");
    let Ok(entries) = std::fs::read_dir(&pages) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path().join("store.id");
        if let Ok(bytes) = std::fs::read(&path)
            && bytes.len() == 16
        {
            let mut id = [0u8; 16];
            id.copy_from_slice(&bytes);
            ids.push(id);
        }
    }
    ids
}

/// Removes every page store folder under this plugin's data directory.
pub fn clear_stores(data: &Path) {
    let _ = std::fs::remove_dir_all(data.join("pages"));
}

/// Deletes `pages/<page>/` only when its `store.id` is in `removed`. Keeps the others so a later
/// start can ask `WebKit` again.
pub fn clear_removed_stores(data: &Path, removed: &[[u8; 16]]) {
    if removed.is_empty() {
        return;
    }
    let pages = data.join("pages");
    let Ok(entries) = std::fs::read_dir(&pages) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let id_path = path.join("store.id");
        let Ok(bytes) = std::fs::read(&id_path) else {
            continue;
        };
        if bytes.len() != 16 {
            continue;
        }
        let mut id = [0u8; 16];
        id.copy_from_slice(&bytes);
        if removed.contains(&id) {
            let _ = std::fs::remove_dir_all(&path);
        }
    }
    if std::fs::read_dir(&pages).is_ok_and(|mut entries| entries.next().is_none()) {
        let _ = std::fs::remove_dir(&pages);
    }
}

/// The ids that were deleted, and a warning for each id that was not.
pub struct StoreRemoval {
    pub removed: Vec<[u8; 16]>,
    pub warnings: Vec<String>,
}

/// How long a store delete waits for GPUI to drop the native `WebView` and frame mouse listener.
pub(crate) const RELEASE_WAIT: std::time::Duration = std::time::Duration::from_millis(50);
// Longest time spent retrying `InUse`, not counting the first wait.
const RETRY_BUDGET: std::time::Duration = std::time::Duration::from_secs(3);
const RETRY_START: std::time::Duration = std::time::Duration::from_millis(50);
const RETRY_MAX: std::time::Duration = std::time::Duration::from_millis(250);

/// One attempt's outcome for a single store id.
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

/// Installs a fake `remove_data_store` for tests. Cleared when the guard drops.
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

/// Waits for the native view to release, then deletes each store, retrying `DataStoreInUse` with
/// backoff for a few seconds. Ids still held stay out of `removed` so their `store.id` files remain
/// for a later start.
pub(crate) async fn remove_web_stores_after_release(
    ids: Vec<[u8; 16]>,
    cx: &mut gpui_kit::AsyncApp,
) -> StoreRemoval {
    if ids.is_empty() {
        return StoreRemoval {
            removed: Vec::new(),
            warnings: Vec::new(),
        };
    }
    #[cfg(target_os = "macos")]
    if macos_major() < 14 {
        // Custom stores need macOS 14+. Older systems never created one; drop the id files.
        return StoreRemoval {
            removed: ids,
            warnings: Vec::new(),
        };
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
        let still: Vec<[u8; 16]> = attempts
            .into_iter()
            .filter_map(|(id, attempt)| match attempt {
                StoreAttempt::Removed => {
                    removed.push(id);
                    None
                }
                #[cfg(any(target_os = "macos", test))]
                StoreAttempt::InUse => Some(id),
                #[cfg(any(target_os = "macos", test))]
                StoreAttempt::Failed(message) => {
                    warnings.push(message);
                    None
                }
            })
            .collect();
        if still.is_empty() || waited >= RETRY_BUDGET + RELEASE_WAIT {
            for _ in &still {
                warnings.push(
                    "the plugin is removed, but WebKit still held a page store (DataStoreInUse)"
                        .into(),
                );
            }
            // Keep store.id for the ones still in use.
            return StoreRemoval { removed, warnings };
        }
        cx.background_executor().timer(delay).await;
        waited = waited.saturating_add(delay);
        delay = (delay.saturating_mul(2)).min(RETRY_MAX);
        pending = still;
    }
}

async fn remove_one_store(id: [u8; 16], cx: &mut gpui_kit::AsyncApp) -> StoreAttempt {
    #[cfg(test)]
    if let Some(attempt) = REMOVE_HOOK.with(|cell| cell.borrow_mut().as_mut().map(|hook| hook(id)))
    {
        return attempt;
    }
    remove_one_native(id, cx).await
}

#[cfg(target_os = "macos")]
async fn remove_one_native(id: [u8; 16], cx: &mut gpui_kit::AsyncApp) -> StoreAttempt {
    use wry::WebViewExtDarwin as _;

    let (send, receive) = futures::channel::oneshot::channel();
    cx.update(|_| {
        wry::WebView::remove_data_store(&id, move |result| {
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
    _cx: &mut gpui_kit::AsyncApp,
) -> impl std::future::Future<Output = StoreAttempt> {
    std::future::ready(StoreAttempt::Removed)
}

/// Deletes each store once, with no wait or retry. The host-start orphan retry uses it.
pub(crate) async fn remove_web_stores(
    ids: Vec<[u8; 16]>,
    cx: &mut gpui_kit::AsyncApp,
) -> StoreRemoval {
    if ids.is_empty() {
        return StoreRemoval {
            removed: Vec::new(),
            warnings: Vec::new(),
        };
    }
    #[cfg(target_os = "macos")]
    if macos_major() < 14 {
        return StoreRemoval {
            removed: ids,
            warnings: Vec::new(),
        };
    }
    let mut attempts = Vec::new();
    for id in ids {
        attempts.push((id, remove_one_store(id, cx).await));
    }
    let mut removed = Vec::new();
    let warnings = attempts
        .into_iter()
        .filter_map(|(id, attempt)| match attempt {
            StoreAttempt::Removed => {
                removed.push(id);
                None
            }
            #[cfg(any(target_os = "macos", test))]
            StoreAttempt::InUse => Some(
                "the plugin is removed, but WebKit still held a page store (DataStoreInUse)".into(),
            ),
            #[cfg(any(target_os = "macos", test))]
            StoreAttempt::Failed(message) => Some(message),
        })
        .collect();
    StoreRemoval { removed, warnings }
}

/// Retries `WebKit` store deletion for plugin data folders that are no longer installed. A prior
/// remove may have left `store.id` when `WebKit` still held the view.
pub(crate) async fn retry_orphan_stores(
    data_dir: &Path,
    known: &std::collections::BTreeSet<kg_domain::PluginId>,
    cx: &mut gpui_kit::AsyncApp,
) {
    let plugins = data_dir.join("plugins");
    let Ok(entries) = std::fs::read_dir(&plugins) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        if known.contains(&kg_domain::PluginId(name.into())) {
            continue;
        }
        let data = entry.path().join("data");
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
pub(crate) fn macos_major() -> u32 {
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
        clear_stores(data.path());
        assert!(!data.path().join("pages").exists());
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
