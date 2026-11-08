use std::io::{Read as _, Seek as _};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use cap_std::ambient_authority;
use cap_std::fs::Dir;
use gpui_kit::AppContext as _;
use gpui_shell::{HostArguments, HostError, HostModule, HostValue};
use serde_json::json;

use crate::area::{awaited, to_host, with_app};

const MAX_BYTES: u64 = 1024 * 1024;
const MAX_LIST_ENTRIES: usize = 10_000;
const MAX_LIST_NAME_BYTES: usize = 1024 * 1024;

pub(crate) const DECLARATIONS: &str = "export function read(path: string, options: { offset: number; maxBytes: number }): Promise<{ text: string; next: number; size: number }>;\nexport function list(dir: string): Promise<{ path: string; size: number; modifiedMs: number }[]>;";

#[derive(Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{path}: {source}", path = .path.display())]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path} is not valid UTF-8 at {offset}", path = .path.display())]
    NotUtf8 { path: PathBuf, offset: u64 },
    #[error("read is too long")]
    TooLong,
    #[error("maxBytes does not fit")]
    MaxBytes,
    #[error("{} is not a granted folder", .0.display())]
    NotGranted(PathBuf),
    #[error("{} is not a directory", .0.display())]
    NotDirectory(PathBuf),
    #[error("{} changed while it was opened", .0.display())]
    Changed(PathBuf),
    #[error(
        "directory exceeded the {MAX_LIST_ENTRIES}-entry or {MAX_LIST_NAME_BYTES}-name-byte list limit"
    )]
    ListLimit,
    #[error("{0}")]
    IoMessage(std::io::Error),
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::IoMessage(source)
    }
}

pub(crate) fn module(
    roots: Option<&[PathBuf]>,
    id: &kg_domain::PluginId,
    secrets: &crate::secret::Store,
) -> HostModule {
    let module = HostModule::new("kagan/files").declarations(DECLARATIONS);
    let Some(roots) = roots else {
        return module;
    };
    let id = id.clone();
    let secrets = secrets.clone();
    let held = Arc::new(open_roots(roots, |error| {
        secrets.note(&id, crate::log::Level::Error, &error.to_string());
    }));
    let list_roots = Arc::clone(&held);
    module
        .async_function("read", move |arguments| {
            let path = arguments.string(0)?.to_owned();
            let (offset, max_bytes) = range(arguments)?;
            let roots = Arc::clone(&held);
            spawn(move || read_at(&roots, Path::new(&path), offset, max_bytes))
        })
        .async_function("list", move |arguments| {
            let path = arguments.string(0)?.to_owned();
            let roots = Arc::clone(&list_roots);
            spawn(move || list_dir(&roots, Path::new(&path)))
        })
}

fn spawn(
    work: impl FnOnce() -> Result<serde_json::Value, Error> + Send + 'static,
) -> Result<impl std::future::Future<Output = Result<HostValue, HostError>> + Send, HostError> {
    let read = with_app(|cx| awaited(cx.background_spawn(async move { work() }), cx))?;
    Ok(async move {
        read.await?
            .map(to_host)
            .map_err(|error| HostError::new(error.to_string()))
    })
}

fn range(arguments: &HostArguments) -> Result<(u64, u64), HostError> {
    let options = arguments
        .get(1)
        .ok_or_else(|| HostError::new("read expects { offset, maxBytes }"))?;
    let offset = whole(options.get("offset"))?;
    let max_bytes = whole(options.get("maxBytes"))?;
    if max_bytes > MAX_BYTES {
        return Err(HostError::new(format!(
            "maxBytes is {max_bytes}, over the {MAX_BYTES}-byte limit"
        )));
    }
    Ok((offset, max_bytes))
}

fn whole(value: Option<&HostValue>) -> Result<u64, HostError> {
    let number = value
        .and_then(HostValue::as_number)
        .filter(|number| {
            number.is_finite()
                && *number >= 0.0
                && number.fract() == 0.0
                && *number <= 9_007_199_254_740_992.0
        })
        .ok_or_else(|| HostError::new("offset and maxBytes must be whole numbers"))?;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "filtered to a finite whole number from 0 through 2^53, which fits in u64"
    )]
    Ok(number as u64)
}

fn read_at(
    roots: &[Root],
    path: &Path,
    offset: u64,
    max_bytes: u64,
) -> Result<serde_json::Value, Error> {
    let opened = accept(roots, path)?;
    read_opened(&opened, offset, max_bytes)
}

fn read_opened(
    opened: &Opened<'_>,
    offset: u64,
    max_bytes: u64,
) -> Result<serde_json::Value, Error> {
    let at = |source| Error::Io {
        path: opened.absolute.clone(),
        source,
    };
    let mut file = opened.dir.open(&opened.relative).map_err(at)?;
    let size = file.metadata().map_err(at)?.len();
    if offset >= size || max_bytes == 0 {
        return Ok(json!({ "text": "", "next": offset.min(size), "size": size }));
    }
    file.seek(std::io::SeekFrom::Start(offset)).map_err(at)?;
    let len = usize::try_from(max_bytes).map_err(|_| Error::MaxBytes)?;
    let mut bytes = vec![0; len];
    let read = file.read(&mut bytes).map_err(at)?;
    bytes.truncate(read);
    let end = match std::str::from_utf8(&bytes) {
        Ok(text) => text.len(),
        Err(error) => error.valid_up_to(),
    };
    if end == 0 && read > 0 {
        return Err(Error::NotUtf8 {
            path: opened.absolute.clone(),
            offset,
        });
    }
    bytes.truncate(end);
    let text = String::from_utf8(bytes).map_err(|error| {
        Error::IoMessage(std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    })?;
    let next = offset + u64::try_from(end).map_err(|_| Error::TooLong)?;
    Ok(json!({ "text": text, "next": next, "size": size }))
}

fn list_dir(roots: &[Root], path: &Path) -> Result<serde_json::Value, Error> {
    let opened = accept(roots, path)?;
    let at = |source| Error::Io {
        path: opened.absolute.clone(),
        source,
    };
    let mut rows = Vec::new();
    let mut name_bytes = 0;
    for entry in opened.dir.read_dir(&opened.relative).map_err(at)? {
        let entry = entry.map_err(at)?;
        let name = entry.file_name();
        name_bytes += name.len();
        if rows.len() == MAX_LIST_ENTRIES || name_bytes > MAX_LIST_NAME_BYTES {
            return Err(Error::ListLimit);
        }
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.into_std().duration_since(UNIX_EPOCH).ok())
            .and_then(|time| u64::try_from(time.as_millis()).ok())
            .unwrap_or(0);
        rows.push(json!({
            "path": opened.absolute.join(&name),
            "size": metadata.len(),
            "modifiedMs": modified,
        }));
    }
    rows.sort_by(|left, right| left["path"].as_str().cmp(&right["path"].as_str()));
    Ok(serde_json::Value::Array(rows))
}

struct Root {
    dir: Dir,
    path: PathBuf,
}

struct Opened<'a> {
    dir: &'a Dir,
    relative: PathBuf,
    absolute: PathBuf,
}

fn open_roots(roots: &[PathBuf], mut fail: impl FnMut(&Error)) -> Vec<Root> {
    let mut opened = Vec::new();
    for root in roots {
        match hold(root) {
            Ok(root) => opened.push(root),
            Err(error) => fail(&error),
        }
    }
    opened
}

// The handle is the grant. A later swap of `root` cannot redirect reads.
fn hold(root: &Path) -> Result<Root, Error> {
    let path = dunce::canonicalize(root)?;
    let marked = directory_key(&path)?;
    let dir = open_grant(&path)?;
    let opened = held_key(&dir)?;
    if marked != opened {
        return Err(Error::Changed(path));
    }
    Ok(Root { dir, path })
}

fn accept<'a>(roots: &'a [Root], path: &Path) -> Result<Opened<'a>, Error> {
    for root in roots {
        let Some((relative, absolute)) = relative_inside(&root.path, path) else {
            continue;
        };
        return Ok(Opened {
            dir: &root.dir,
            relative,
            absolute,
        });
    }
    Err(Error::NotGranted(path.to_path_buf()))
}

fn relative_inside(root: &Path, path: &Path) -> Option<(PathBuf, PathBuf)> {
    let resolved = normalize(&resolved_path(path)?);
    let relative = resolved.strip_prefix(root).ok()?.to_path_buf();
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_) | Component::CurDir))
    {
        return None;
    }
    let relative = if relative.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        relative
    };
    let absolute = if relative == Path::new(".") {
        root.to_path_buf()
    } else {
        root.join(&relative)
    };
    Some((relative, absolute))
}

fn open_grant(path: &Path) -> Result<Dir, Error> {
    // cap-std lists a directory by turning this handle back into a path.
    // `open_ambient_dir` omits `FILE_SHARE_DELETE`, so the name cannot be
    // removed and reused while the grant is held.
    Dir::open_ambient_dir(path, ambient_authority()).map_err(Error::IoMessage)
}

#[cfg(unix)]
fn directory_key(path: &Path) -> Result<(u64, u64), Error> {
    use std::os::unix::fs::MetadataExt;
    let marked = std::fs::symlink_metadata(path)?;
    if !real_directory(&marked) {
        return Err(Error::NotDirectory(path.to_path_buf()));
    }
    Ok((marked.dev(), marked.ino()))
}

#[cfg(unix)]
fn held_key(dir: &Dir) -> std::io::Result<(u64, u64)> {
    use std::os::fd::{AsFd, OwnedFd};
    use std::os::unix::fs::MetadataExt;
    let owned: OwnedFd = dir.as_fd().try_clone_to_owned()?;
    let opened = std::fs::File::from(owned).metadata()?;
    Ok((opened.dev(), opened.ino()))
}

#[cfg(windows)]
fn directory_key(path: &Path) -> Result<(u32, u64), Error> {
    let file = open_no_follow(path)?;
    let marked = file.metadata()?;
    if !real_directory(&marked) {
        return Err(Error::NotDirectory(path.to_path_buf()));
    }
    Ok(file_key(&file)?)
}

#[cfg(windows)]
fn open_no_follow(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    };
    std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(windows)]
fn held_key(dir: &Dir) -> std::io::Result<(u32, u64)> {
    use std::os::windows::io::{AsHandle, OwnedHandle};
    let owned: OwnedHandle = dir.as_handle().try_clone_to_owned()?;
    file_key(&std::fs::File::from(owned))
}

#[cfg(windows)]
fn file_key(file: &std::fs::File) -> std::io::Result<(u32, u64)> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
    };
    // SAFETY: every field is a plain integer, so zero is a valid out-parameter.
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    // SAFETY: `info` is initialized and the handle belongs to `file`.
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Ok((info.dwVolumeSerialNumber, index))
}

fn real_directory(metadata: &std::fs::Metadata) -> bool {
    metadata.is_dir() && !metadata.file_type().is_symlink()
}

fn resolved_path(path: &Path) -> Option<PathBuf> {
    let mut tail = Vec::new();
    let mut current = path.to_path_buf();
    let resolved = loop {
        if let Ok(real) = dunce::canonicalize(&current) {
            break real;
        }
        let name = current.file_name()?.to_os_string();
        if !current.pop() {
            return None;
        }
        tail.push(name);
    };
    tail.reverse();
    let mut out = resolved;
    out.extend(tail);
    Some(out)
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use std::path::Path;

    use super::{Error, hold, list_dir, open_roots};
    #[cfg(unix)]
    use super::{accept, read_at, read_opened};

    #[cfg(unix)]
    fn unread(result: &Result<serde_json::Value, Error>, secret: &str) {
        match result {
            Ok(value) => assert!(!value.to_string().contains(secret), "{value}"),
            Err(error) => assert!(!error.to_string().contains(secret), "{error}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn an_in_root_symlink_to_a_sibling_is_refused() {
        let sandbox = tempfile::tempdir().unwrap();
        let allowed = sandbox.path().join("allowed");
        let sibling = sandbox.path().join("sibling");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("secret.txt"), "secret-sibling").unwrap();
        std::os::unix::fs::symlink(&sibling, allowed.join("escape")).unwrap();
        let roots = open_roots(std::slice::from_ref(&allowed), |_| {});
        let through = allowed.join("escape/secret.txt");
        unread(&read_at(&roots, &through, 0, 64), "secret-sibling");
        unread(&list_dir(&roots, &allowed.join("escape")), "secret-sibling");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_planted_after_the_path_was_accepted_is_still_refused() {
        let sandbox = tempfile::tempdir().unwrap();
        let allowed = sandbox.path().join("allowed");
        let sibling = sandbox.path().join("sibling");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("secret.txt"), "secret-sibling").unwrap();
        let roots = open_roots(std::slice::from_ref(&allowed), |_| {});
        let opened = accept(&roots, &allowed.join("escape/secret.txt")).unwrap();
        std::os::unix::fs::symlink(&sibling, allowed.join("escape")).unwrap();
        unread(&read_opened(&opened, 0, 64), "secret-sibling");
    }

    #[cfg(unix)]
    fn replaced_grant_is_unread(replace: fn(&Path, &Path)) {
        let sandbox = tempfile::tempdir().unwrap();
        let allowed = sandbox.path().join("allowed");
        let sibling = sandbox.path().join("sibling");
        std::fs::create_dir_all(&allowed).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(sibling.join("secret.txt"), "secret-outside").unwrap();
        let roots = open_roots(std::slice::from_ref(&allowed), |_| {});
        assert_eq!(roots.len(), 1);
        replace(&allowed, &sibling);
        unread(
            &read_at(&roots, &allowed.join("secret.txt"), 0, 64),
            "secret-outside",
        );
        unread(&list_dir(&roots, &allowed), "secret-outside");
    }

    #[cfg(unix)]
    #[test]
    fn a_granted_directory_replaced_by_a_symlink_is_refused() {
        replaced_grant_is_unread(|dir, target| {
            std::fs::remove_dir(dir).unwrap();
            std::os::unix::fs::symlink(target, dir).unwrap();
        });
    }

    #[cfg(windows)]
    #[test]
    fn a_granted_directory_replaced_by_a_junction_is_refused() {
        let sandbox = tempfile::tempdir().unwrap();
        let allowed = sandbox.path().join("allowed");
        std::fs::create_dir_all(&allowed).unwrap();
        let roots = open_roots(std::slice::from_ref(&allowed), |_| {});
        assert_eq!(roots.len(), 1);
        assert!(
            std::fs::remove_dir(&allowed).is_err(),
            "the grant let the directory be removed"
        );
        std::fs::write(allowed.join("marker.txt"), "inside").unwrap();
        let listed = list_dir(&roots, &allowed).unwrap().to_string();
        assert!(listed.contains("marker.txt"), "{listed}");
        drop(roots);
        std::fs::remove_file(allowed.join("marker.txt")).unwrap();
        std::fs::remove_dir(&allowed).unwrap();
    }

    #[test]
    fn a_file_is_not_a_directory_grant() {
        let sandbox = tempfile::tempdir().unwrap();
        let file = sandbox.path().join("note.txt");
        std::fs::write(&file, "secret-file").unwrap();
        let Err(error) = hold(&file) else {
            panic!("a file opened as a directory");
        };
        assert!(matches!(&error, super::Error::NotDirectory(_)), "{error}");
        let error = error.to_string();
        assert!(error.contains("is not a directory"), "{error}");
        assert!(!error.contains("secret-file"), "{error}");
    }

    #[test]
    fn a_root_that_fails_to_open_is_written_to_the_plugin_log() {
        let data = tempfile::tempdir().unwrap();
        let missing = data.path().join("missing");
        let id = kg_domain::PluginId("dev.test.echo".into());
        let Err(reason) = hold(&missing) else {
            panic!("missing folder opened");
        };
        let roots = open_roots(std::slice::from_ref(&missing), |error| {
            crate::log(
                data.path(),
                &id,
                crate::log::Level::Error,
                &error.to_string(),
            )
            .unwrap();
        });
        assert!(roots.is_empty());
        let text = std::fs::read_to_string(kg_core::plugin_log_file(data.path(), &id)).unwrap();
        assert!(text.contains(&reason.to_string()), "{text}");
    }
}
