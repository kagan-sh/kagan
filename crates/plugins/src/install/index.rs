use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use gpui_kit::{AppContext as _, AsyncApp, Entity};
use kg_domain::{GIT_SOURCE, IndexEntry, IndexOp, Listing, OpError, OpResult, is_git_source};

use crate::manifest::inside;
use crate::source::{Index, Source, clone, git};
use crate::{PluginHost, Result};

const FILE: &str = ".kagan/index.json";
const SAVED: &str = "indexes.json";
const LIMIT: usize = 1024 * 1024;
const DESCRIPTION: usize = 300;

/// Index and plugin names show on the approval card, so only plain lowercase ASCII is allowed:
/// no line breaks that could fake card lines, and no lookalike letters.
fn plain(name: &str) -> bool {
    (1..=64).contains(&name.len())
        && name.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"-_.".contains(&byte)
        })
}

/// Where a `git:` source lives: its host and owner, lowercased. The owner is the first part of
/// the path, in a URL or in `user@host:owner/repo`.
pub(crate) fn source_place(source: &str) -> Option<(String, String)> {
    let Ok(Source::Git { url, .. }) = Source::parse(source) else {
        return None;
    };
    let (host, path) = if let Ok(parsed) = url::Url::parse(&url) {
        let host = parsed.host_str().filter(|host| !host.is_empty())?;
        (host.to_owned(), parsed.path().to_owned())
    } else {
        let (user_host, path) = url.split_once(':')?;
        let host = user_host.rsplit('@').next()?;
        if host.len() < 2 || host.contains('/') {
            return None;
        }
        (host.to_owned(), path.to_owned())
    };
    let owner = path
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or_default();
    Some((host.to_lowercase(), owner.to_lowercase()))
}

/// What an approval card says about where code comes from: the address, ref and full commit,
/// the index that listed it, and a caution unless an index the person added lives at the same
/// host and owner.
pub(crate) fn origin(
    source: &str,
    root: &Path,
    commit: Option<&str>,
    listed: Option<&Listing>,
    indexes: &[String],
) -> Vec<String> {
    let by = listed.map_or(String::new(), |listing| {
        format!(
            ", listed by index {} at {}",
            listing.index, listing.location
        )
    });
    let Ok(Source::Git { url, reference }) = Source::parse(source) else {
        return vec![
            format!("From the folder {}{by}.", root.display()),
            "Caution: a folder on this computer is not pinned to a commit.".into(),
        ];
    };
    let from = format!(
        "From {} at {reference}, commit {}{by}.",
        public(&url),
        commit.unwrap_or_default()
    );
    let caution = match source_place(source) {
        Some(place)
            if indexes
                .iter()
                .any(|index| self::source_place(index).as_ref() == Some(&place)) =>
        {
            return vec![from];
        }
        Some((host, owner)) => {
            format!("Caution: no index you added lives at {host}/{owner}.")
        }
        None => "Caution: this repository is on this computer, not where an index you added lives."
            .into(),
    };
    vec![from, caution]
}

/// `url` without a password, which may be a token. An http or https user name goes too; an ssh
/// user name such as `git` is part of the address.
pub(crate) fn public(url: &str) -> String {
    match url::Url::parse(url) {
        Ok(mut parsed) if parsed.has_host() => {
            _ = parsed.set_password(None);
            if matches!(parsed.scheme(), "http" | "https") {
                _ = parsed.set_username("");
            }
            String::from(parsed)
        }
        _ => url.to_owned(),
    }
}

/// An index location as people and agents see it: a `git:` address without a password.
#[must_use]
pub fn shown(location: &str) -> String {
    match Source::parse(location) {
        Ok(Source::Git { url, reference }) => format!("{GIT_SOURCE}{}@{reference}", public(&url)),
        _ => location.to_owned(),
    }
}

/// A source that is neither `git:` nor an absolute path names a plugin in the indexes.
pub(crate) fn names_an_index(source: &str) -> bool {
    !is_git_source(source) && Path::new(source).is_relative()
}

/// Why the saved index list cannot be read.
#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum Error {
    #[error("{path}: {source}", path = .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: std::sync::Arc<std::io::Error>,
    },
    #[error("{path}: {source}", path = .path.display())]
    Parse {
        path: PathBuf,
        #[source]
        source: std::sync::Arc<serde_json::Error>,
    },
}

/// The saved index locations. A missing file is no indexes; a file that cannot be read is an
/// error, so nothing saves over it.
pub(crate) fn load(data_dir: &Path) -> std::result::Result<Vec<String>, Error> {
    let path = data_dir.join(SAVED);
    match std::fs::read_to_string(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(source) => Err(Error::Read {
            path,
            source: std::sync::Arc::new(source),
        }),
        Ok(text) => serde_json::from_str(&text).map_err(|source| Error::Parse {
            path,
            source: std::sync::Arc::new(source),
        }),
    }
}

pub(crate) fn locations(saved: &std::result::Result<Vec<String>, Error>) -> Result<Vec<String>> {
    saved.clone().map_err(|error| {
        OpError::Failed(format!(
            "the list of indexes cannot be read; fix or delete it: {error}"
        ))
    })
}

fn save(data_dir: &Path, locations: &[String]) -> Result<()> {
    let mut file = tempfile::NamedTempFile::new_in(data_dir)?;
    serde_json::to_writer_pretty(&mut file, locations)?;
    file.persist(data_dir.join(SAVED))
        .map_err(|error| OpError::Failed(error.to_string()))?;
    Ok(())
}

/// Reads the index at `location`: its name and listings. A git repository and a local folder
/// hold it at `.kagan/index.json`; a local file is the index itself.
pub(crate) fn read(
    location: &str,
    stop: &async_channel::Receiver<()>,
) -> Result<(String, Vec<Listing>)> {
    if location.starts_with("http://") || location.starts_with("https://") {
        return Err(OpError::Invalid(
            "an index address is git:<url>@<ref> or a folder; kagan does not read a plain web file"
                .into(),
        ));
    }
    let (name, entries) = match Source::parse(location)? {
        Source::Git { url, reference } => {
            let checkout = tempfile::tempdir()?;
            let commit = clone(&url, &reference, checkout.path(), stop)?;
            let blob = format!("{commit}:{FILE}");
            let run = |args: &[&str]| git(Some(checkout.path()), args.iter().map(OsStr::new), stop);
            let size: usize = run(&["cat-file", "-s", &blob])?
                .parse()
                .map_err(|_| OpError::Invalid(format!("{FILE} is not a file")))?;
            if size > LIMIT {
                return Err(OpError::Invalid(format!(
                    "the index is larger than {} MiB",
                    LIMIT >> 20
                )));
            }
            parse(&run(&["show", &blob])?, None)?
        }
        Source::Local(path) => {
            let file = if path.is_dir() { path.join(FILE) } else { path };
            let mut text = String::new();
            std::fs::File::open(&file)
                .map_err(|error| OpError::Failed(format!("{}: {error}", file.display())))?
                .take(LIMIT as u64 + 1)
                .read_to_string(&mut text)?;
            let folder = file.parent().map(|parent| {
                if parent.file_name() == Some(OsStr::new(".kagan")) {
                    parent.parent().unwrap_or(parent)
                } else {
                    parent
                }
            });
            parse(&text, folder)?
        }
    };
    let location = shown(location);
    let listings = entries
        .into_iter()
        .map(|entry| Listing {
            index: name.clone(),
            location: location.clone(),
            name: entry.name,
            description: entry.description,
            source: entry.source,
            plugin: entry.plugin,
        })
        .collect();
    Ok((name, listings))
}

/// The index's name and entries. `folder` is where a local index's paths start, and they come
/// back absolute; an index read from git lists only git sources.
pub(crate) fn parse(text: &str, folder: Option<&Path>) -> Result<(String, Vec<IndexEntry>)> {
    if text.len() > LIMIT {
        return Err(OpError::Invalid(format!(
            "the index is larger than {} MiB",
            LIMIT >> 20
        )));
    }
    let index: Index<IndexEntry> =
        serde_json::from_str(text).map_err(|error| OpError::Invalid(format!("{FILE}: {error}")))?;
    if index.schema_version != 1 || !plain(&index.name) {
        return Err(OpError::Invalid(format!(
            "{FILE} needs schemaVersion 1 and a name of up to 64 lowercase letters, digits, - _ or ."
        )));
    }
    let mut names = BTreeSet::new();
    let entries = index
        .plugins
        .into_iter()
        .map(|mut entry| {
            let refuse = |why: &str| {
                OpError::Invalid(format!(
                    "index {}: plugin {:?} {why}",
                    index.name, entry.name
                ))
            };
            if !plain(&entry.name) {
                return Err(refuse(
                    "needs a name of up to 64 lowercase letters, digits, - _ or .",
                ));
            }
            if !names.insert(entry.name.clone()) {
                return Err(refuse("is listed twice"));
            }
            if entry.description.chars().count() > DESCRIPTION
                || entry.description.chars().any(char::is_control)
            {
                return Err(refuse(
                    "needs a description of up to 300 characters on one line",
                ));
            }
            entry.source = match (Source::parse(&entry.source), folder) {
                (Ok(Source::Git { .. }), _) => entry.source.clone(),
                (Ok(Source::Local(path)), Some(folder)) if !entry.source.is_empty() => {
                    inside(folder, &path)
                        .ok_or_else(|| refuse("must be a path inside the index's folder"))?
                        .display()
                        .to_string()
                }
                (Ok(Source::Local(_)), _) => {
                    return Err(refuse("needs a source git:<url>@<ref>"));
                }
                (Err(error), _) => return Err(refuse(&error.to_string())),
            };
            Ok(entry)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((index.name, entries))
}

/// A name the indexes list becomes that listing's source and `--plugin`; another source stays.
pub(crate) fn resolve(
    indexes: &std::result::Result<Vec<String>, Error>,
    source: String,
    plugin: Option<String>,
    stop: &async_channel::Receiver<()>,
) -> Result<(String, Option<String>, Option<Listing>)> {
    if !names_an_index(&source) {
        return Ok((source, plugin, None));
    }
    if plugin.is_some() {
        return Err(OpError::Invalid(format!(
            "--plugin is for a git address or a folder; the index names {source}'s folder"
        )));
    }
    let listing = find(&locations(indexes)?, &source, stop)?;
    Ok((
        listing.source.clone(),
        listing.plugin.clone(),
        Some(listing),
    ))
}

/// The listing `wanted` names, `<plugin>` or `<index>/<plugin>`, read in order. An index that
/// cannot be read stops the search, so a later one cannot stand in for it. `<index>` is the
/// first index with that name, since an index names itself.
pub(crate) fn find(
    locations: &[String],
    wanted: &str,
    stop: &async_channel::Receiver<()>,
) -> Result<Listing> {
    let (index, name) = match wanted.split_once('/') {
        Some((index, name)) => (Some(index), name),
        None => (None, wanted),
    };
    let missing = || {
        OpError::NotFound(if locations.is_empty() {
            format!("no folder {wanted}, and no index is added; add one with kg plugin index add")
        } else {
            format!("no folder {wanted}, and no index lists a plugin named {wanted}")
        })
    };
    for location in locations {
        let (named, listings) = read(location, stop).map_err(|error| {
            OpError::Failed(format!("could not read index {}: {error}", shown(location)))
        })?;
        if index.is_some_and(|index| index != named) {
            continue;
        }
        let found = listings.into_iter().find(|listing| listing.name == name);
        if found.is_some() || index.is_some() {
            return found.ok_or_else(missing);
        }
    }
    Err(missing())
}

fn already(location: &str) -> OpError {
    OpError::Invalid(format!("index {} is already added", shown(location)))
}

pub(crate) async fn run(
    host: &Entity<PluginHost>,
    op: IndexOp,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let saved = host.read_with(cx, |host, _| locations(&host.indexes))?;
    if let IndexOp::Add { location } = &op {
        if saved.contains(location) {
            return Err(already(location));
        }
        let (location, saved) = (location.clone(), saved.clone());
        cx.background_spawn(async move {
            let (_keep, never) = async_channel::bounded::<()>(1);
            let (name, _) = read(&location, &never)?;
            for known in &saved {
                if read(known, &never).is_ok_and(|(other, _)| other == name) {
                    return Err(OpError::Invalid(format!(
                        "index {} is already named {name}; two indexes cannot share a name",
                        shown(known)
                    )));
                }
            }
            Ok(())
        })
        .await?;
    }
    host.update(cx, |host, cx| {
        let mut saved = locations(&host.indexes)?;
        match op {
            IndexOp::List => {}
            IndexOp::Add { location } if saved.contains(&location) => {
                return Err(already(&location));
            }
            IndexOp::Add { location } => saved.push(location),
            IndexOp::Remove { location } => {
                let at = saved
                    .iter()
                    .position(|known| *known == location || shown(known) == location)
                    .ok_or_else(|| {
                        OpError::NotFound(format!("index {} is not added", shown(&location)))
                    })?;
                saved.remove(at);
            }
        }
        if host.indexes.as_ref().ok().map(Vec::as_slice) != Some(saved.as_slice()) {
            save(&host.data_dir, &saved)?;
            host.indexes = Ok(saved.clone());
            cx.notify();
        }
        Ok(OpResult::Indexes(
            saved.iter().map(|known| shown(known)).collect(),
        ))
    })
}

/// Listings whose name or description holds `text`, ignoring case. An index that cannot be
/// read, or that reuses an earlier index's name, becomes a warning, and the others still answer.
pub(crate) async fn search(
    host: &Entity<PluginHost>,
    text: String,
    cx: &mut AsyncApp,
) -> Result<OpResult> {
    let saved = host.read_with(cx, |host, _| locations(&host.indexes))?;
    let text = text.to_lowercase();
    let (listings, warnings) = cx
        .background_spawn(async move {
            let (_keep, never) = async_channel::bounded::<()>(1);
            let mut names = BTreeSet::new();
            let mut listings = Vec::new();
            let mut warnings = Vec::new();
            for location in &saved {
                match read(location, &never) {
                    Ok((name, _)) if !names.insert(name.clone()) => warnings.push(format!(
                        "index {} is also named {name}; only the first is used",
                        shown(location)
                    )),
                    Ok((_, found)) => listings.extend(found.into_iter().filter(|listing| {
                        listing.name.to_lowercase().contains(&text)
                            || listing.description.to_lowercase().contains(&text)
                    })),
                    Err(error) => {
                        warnings.push(format!("could not read index {}: {error}", shown(location)));
                    }
                }
            }
            (listings, warnings)
        })
        .await;
    Ok(OpResult::Listings { listings, warnings })
}

#[cfg(test)]
mod tests {
    use super::{Error, SAVED, load};

    #[test]
    fn an_unreadable_saved_list_is_a_typed_error_naming_the_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SAVED), "{").unwrap();
        let error = load(dir.path()).unwrap_err();
        assert!(matches!(&error, Error::Parse { .. }), "{error}");
        assert!(error.to_string().contains("indexes.json"), "{error}");
    }
}
