use std::path::{Component, Path, PathBuf};

/// Why a `powers.read` entry is refused.
#[derive(Debug, thiserror::Error)]
pub(crate) enum ReadRootError {
    #[error("read path {0:?} must be a folder under home, written as ~/")]
    NotHome(String),
    #[error("read path {0:?} must stay inside the home directory")]
    Inside(String),
    #[error("read path {0:?} resolves outside the home directory")]
    Outside(String),
    #[error("read path {entry:?} cannot cover {name}")]
    Covers { entry: String, name: String },
    #[error("read path {0:?} must be a folder")]
    Folder(String),
    #[error("{0} is not set")]
    HomeUnset(&'static str),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

pub(crate) fn read_root(entry: &str, data_dir: &Path) -> Result<Option<PathBuf>, ReadRootError> {
    let rest = entry
        .strip_prefix("~/")
        .ok_or_else(|| ReadRootError::NotHome(entry.to_owned()))?;
    if rest.is_empty()
        || rest.contains('\\')
        || rest.contains(':')
        || !Path::new(rest)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return Err(ReadRootError::Inside(entry.to_owned()));
    }
    let home = home_dir()?;
    let joined = home.join(rest);
    if !joined.starts_with(&home) {
        return Err(ReadRootError::Outside(entry.to_owned()));
    }
    if let Some(error) = denied(&joined, entry, data_dir, &home) {
        return Err(error);
    }
    if !joined.exists() {
        return Ok(None);
    }
    if !joined.is_dir() {
        return Err(ReadRootError::Folder(entry.to_owned()));
    }
    let canonical = dunce::canonicalize(&joined)?;
    let home = match dunce::canonicalize(&home) {
        Ok(path) => path,
        Err(_) => home,
    };
    if let Some(error) = denied(&canonical, entry, data_dir, &home) {
        return Err(error);
    }
    if canonical == home || !canonical.starts_with(&home) {
        return Err(ReadRootError::Outside(entry.to_owned()));
    }
    Ok(Some(canonical))
}

fn denied(path: &Path, entry: &str, data_dir: &Path, home: &Path) -> Option<ReadRootError> {
    let ssh = home.join(".ssh");
    let gnupg = home.join(".gnupg");
    let protected = [
        (data_dir, resolved(data_dir).display().to_string()),
        (ssh.as_path(), "~/.ssh".to_owned()),
        (gnupg.as_path(), "~/.gnupg".to_owned()),
    ];
    protected.into_iter().find_map(|(protected, name)| {
        covers(path, protected).then(|| ReadRootError::Covers {
            entry: entry.to_owned(),
            name,
        })
    })
}

fn covers(path: &Path, protected: &Path) -> bool {
    let path_resolved = resolved(path);
    let protected_resolved = resolved(protected);
    path_resolved.starts_with(&protected_resolved)
        || protected_resolved.starts_with(&path_resolved)
        || path.starts_with(protected)
        || protected.starts_with(path)
}

fn resolved(path: &Path) -> PathBuf {
    dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

fn home_dir() -> Result<PathBuf, ReadRootError> {
    let key = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(key)
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
        .ok_or(ReadRootError::HomeUnset(key))
}

/// `relative` joined to `root`, if it stays inside it.
pub(crate) fn inside(root: &Path, relative: &Path) -> Option<PathBuf> {
    relative
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
        .then(|| root.join(relative))
}
