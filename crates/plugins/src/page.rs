//! A panel's own HTML page: the files it may read, and the JSON bridge to its script.

use std::io::Read as _;
use std::path::{Component, Path, PathBuf};

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
/// The largest file the page may read from the plugin folder.
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

/// The declared `web` file, checked when the manifest is read.
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
) -> Option<(String, async_channel::Receiver<Result<Value, String>>)> {
    let mut job = Map::new();
    job.insert("kind".into(), "page".into());
    job.insert("panelId".into(), panel.into());
    job.insert("message".into(), message);
    jobs.send(job)
}

fn shape(relative: &str) -> Result<(), String> {
    if !is_html(relative)
        || relative.starts_with('/')
        || relative.contains('\\')
        || relative.contains('\0')
        || relative
            .split('/')
            .any(|part| part == RESERVED || part.is_empty())
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
            .any(|part| part.is_empty() || part == RESERVED)
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
    let mut file = dir.open(relative).map_err(|_| PageFileError::Refused)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)
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
