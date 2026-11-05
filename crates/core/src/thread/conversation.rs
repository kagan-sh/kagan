use std::ffi::{OsStr, OsString};
use std::io::BufRead as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, NaiveDate, Utc};
use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{Conversation, Timestamp};
use serde::Deserialize;

use crate::Workspace;
use crate::host::database::Result;
use crate::workspace::closed;

use super::agent::{Known, Start};

// Flags with which the person already picks the conversation, so kagan adds none of its own.
const CLAUDE_FLAGS: &[&str] = &[
    "-c",
    "--continue",
    "-r",
    "--resume",
    "--session-id",
    "--fork-session",
    "--from-pr",
    "--teleport",
    "-p",
    "--print",
    "--no-session-persistence",
];

const CODEX_SUBCOMMANDS: &[&str] = &["resume", "fork"];

const CURSOR_FLAGS: &[&str] = &["--continue", "--new-session-id", "--resume"];

const CODEX_FRESH: &str =
    "kagan could not tell which Codex conversation was this thread's, so Codex starts a new one.";

const NO_CONVERSATION: &str =
    "This thread has no saved conversation to resume, so the agent starts a new one.";

const LOST_CONVERSATION: &str =
    "kagan could not find the agent's last conversation, so it starts a new one.";

enum Last {
    None,
    Gone,
    Kept(Conversation),
}

#[derive(Deserialize)]
struct CodexLine {
    #[serde(rename = "type")]
    kind: String,
    payload: CodexMeta,
}

#[derive(Deserialize)]
struct CodexMeta {
    id: Conversation,
    cwd: PathBuf,
    timestamp: String,
}

#[derive(Deserialize)]
struct CursorMeta {
    cwd: Option<PathBuf>,
}

/// The arguments that resume the thread's conversation or begin one kagan can find again, and a
/// notice when Start again could not resume.
pub(super) async fn args(
    this: &WeakEntity<Workspace>,
    known: &Known,
    record: &kg_domain::Thread,
    args: &[OsString],
    folder: &Path,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<(Vec<OsString>, Option<&'static str>)> {
    match known {
        Known::Claude if picks_claude(args) => Ok((Vec::new(), None)),
        Known::Claude => claude(this, record, start, cx).await,
        Known::Codex if picks_codex(args) || start == Start::First => Ok((Vec::new(), None)),
        Known::Codex if !args.is_empty() => Ok((Vec::new(), Some(CODEX_FRESH))),
        Known::Codex => codex(this, record, folder, cx).await,
        Known::Cursor if picks_cursor(args) => Ok((Vec::new(), None)),
        Known::Cursor => cursor(this, record, folder, start, cx).await,
        Known::Kimi | Known::OpenCode | Known::Pi => Ok((Vec::new(), None)),
    }
}

async fn claude(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<(Vec<OsString>, Option<&'static str>)> {
    let (database, config) = this
        .update(cx, |workspace, _| {
            (workspace.database.clone(), workspace.claude_config.clone())
        })
        .map_err(closed)?;
    let id = record.id.clone();
    let stored = database.call(move |store| store.conversation(&id)).await?;
    let last = match stored {
        None => Last::None,
        Some(conversation) => {
            cx.background_spawn(async move {
                let config = config.or_else(|| home("CLAUDE_CONFIG_DIR", ".claude"));
                if config.is_some_and(|config| claude_kept(&config, conversation)) {
                    Last::Kept(conversation)
                } else {
                    Last::Gone
                }
            })
            .await
        }
    };
    remember(&database, &record.id, last, "--session-id", start).await
}

async fn codex(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    folder: &Path,
    cx: &mut AsyncApp,
) -> Result<(Vec<OsString>, Option<&'static str>)> {
    let (database, codex_home) = this
        .update(cx, |workspace, _| {
            (workspace.database.clone(), workspace.codex_home.clone())
        })
        .map_err(closed)?;
    let id = record.id.clone();
    let (stored, since) = database
        .call(move |store| Ok((store.conversation(&id)?, store.last_started(&id)?)))
        .await?;
    let folder = folder.to_owned();
    let found = cx
        .background_spawn(async move {
            let sessions = codex_home
                .or_else(|| home("CODEX_HOME", ".codex"))?
                .join("sessions");
            stored
                .filter(|conversation| codex_kept(&sessions, *conversation))
                .or_else(|| codex_started(&sessions, &folder, since?))
        })
        .await;
    let Some(conversation) = found else {
        return Ok((Vec::new(), Some(CODEX_FRESH)));
    };
    if stored != Some(conversation) {
        let id = record.id.clone();
        database
            .call(move |store| store.set_conversation(&id, conversation))
            .await?;
    }
    Ok((resume("resume", conversation), None))
}

async fn cursor(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    folder: &Path,
    start: Start,
    cx: &mut AsyncApp,
) -> Result<(Vec<OsString>, Option<&'static str>)> {
    let (database, config) = this
        .update(cx, |workspace, _| {
            (workspace.database.clone(), workspace.cursor_config.clone())
        })
        .map_err(closed)?;
    let id = record.id.clone();
    let stored = database.call(move |store| store.conversation(&id)).await?;
    let folder = folder.to_owned();
    let last = match stored {
        None => Last::None,
        Some(conversation) => {
            let config = config.clone();
            cx.background_spawn(async move {
                let root = cursor_root(config);
                if root.is_some_and(|root| cursor_kept(&root, conversation, &folder)) {
                    Last::Kept(conversation)
                } else {
                    Last::Gone
                }
            })
            .await
        }
    };
    remember(&database, &record.id, last, "--new-session-id", start).await
}

async fn remember(
    database: &crate::host::database::Database,
    thread: &kg_domain::ThreadId,
    last: Last,
    flag: &str,
    start: Start,
) -> Result<(Vec<OsString>, Option<&'static str>)> {
    let notice = match last {
        Last::Kept(conversation) => return Ok((resume("--resume", conversation), None)),
        Last::None => NO_CONVERSATION,
        Last::Gone => LOST_CONVERSATION,
    };
    let conversation = Conversation::fresh();
    let id = thread.clone();
    database
        .call(move |store| store.set_conversation(&id, conversation))
        .await?;
    Ok((
        resume(flag, conversation),
        (start == Start::Again).then_some(notice),
    ))
}

fn resume(word: &str, conversation: Conversation) -> Vec<OsString> {
    vec![OsString::from(word), conversation.to_string().into()]
}

fn picks(args: &[OsString], flags: &[&str]) -> bool {
    args.iter().filter_map(|arg| arg.to_str()).any(|arg| {
        flags.iter().any(|flag| {
            arg.strip_prefix(flag)
                .is_some_and(|rest| rest.is_empty() || rest.starts_with('='))
        })
    })
}

fn picks_claude(args: &[OsString]) -> bool {
    picks(args, CLAUDE_FLAGS)
}

fn picks_cursor(args: &[OsString]) -> bool {
    picks(args, CURSOR_FLAGS)
}

fn picks_codex(args: &[OsString]) -> bool {
    args.iter().any(|arg| {
        CODEX_SUBCOMMANDS
            .iter()
            .any(|word| arg.as_os_str() == OsStr::new(word))
    })
}

fn cursor_root(set: Option<PathBuf>) -> Option<PathBuf> {
    if let Some(set) = set {
        return Some(set);
    }
    if let Some(dir) = std::env::var_os("CURSOR_CONFIG_DIR").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    if let Some(dir) = std::env::var_os("XDG_CONFIG_HOME").filter(|dir| !dir.is_empty()) {
        return Some(PathBuf::from(dir).join("cursor"));
    }
    home("CURSOR_CONFIG_DIR", ".cursor")
}

fn home(variable: &str, folder: &str) -> Option<PathBuf> {
    std::env::var_os(variable)
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" })
                .map(|home| PathBuf::from(home).join(folder))
        })
}

// Claude Code writes a conversation to `projects/<folder>/<id>.jsonl` once something is said, and
// `--resume` with an id it has not written fails. The folder name is Claude's own encoding of the
// working folder, so every folder is searched for the id.
fn claude_kept(config: &Path, conversation: Conversation) -> bool {
    let name = format!("{conversation}.jsonl");
    entries(&config.join("projects")).any(|folder| folder.join(&name).is_file())
}

// Codex writes `sessions/<year>/<month>/<day>/rollout-<local time>-<id>.jsonl`.
fn codex_kept(sessions: &Path, conversation: Conversation) -> bool {
    let end = format!("-{conversation}.jsonl");
    days(sessions, NaiveDate::MIN)
        .flat_map(|day| entries(&day))
        .any(|file| {
            file.file_name()
                .and_then(OsStr::to_str)
                .is_some_and(|name| name.ends_with(&end))
        })
}

// The one conversation Codex began in this folder since the thread's last start. The first line of
// each file names its folder and its start in UTC; the folder names are local dates, so the day
// before the start is read too. Two candidates, or none, find nothing.
fn codex_started(sessions: &Path, folder: &Path, since: Timestamp) -> Option<Conversation> {
    let folder = dunce::canonicalize(folder).ok()?;
    let since = DateTime::<Utc>::from_timestamp_millis(since.0)?;
    let mut found = days(sessions, since.date_naive().pred_opt()?)
        .flat_map(|day| entries(&day))
        .filter(|file| file.extension() == Some(OsStr::new("jsonl")))
        .filter_map(|file| codex_meta(&file))
        .filter(|(meta, started)| {
            *started >= since && dunce::canonicalize(&meta.cwd).is_ok_and(|cwd| cwd == folder)
        })
        .map(|(meta, _)| meta.id);
    let one = found.next()?;
    found.next().is_none().then_some(one)
}

fn codex_meta(file: &Path) -> Option<(CodexMeta, DateTime<Utc>)> {
    let mut line = String::new();
    std::io::BufReader::new(std::fs::File::open(file).ok()?)
        .read_line(&mut line)
        .ok()?;
    let line: CodexLine = serde_json::from_str(&line).ok()?;
    let started = DateTime::parse_from_rfc3339(&line.payload.timestamp).ok()?;
    (line.kind == "session_meta").then(|| (line.payload, started.to_utc()))
}

fn days(sessions: &Path, first: NaiveDate) -> impl Iterator<Item = PathBuf> + use<> {
    entries(sessions)
        .flat_map(|year| entries(&year))
        .flat_map(|month| entries(&month))
        .filter(move |day| date(day).is_some_and(|date| date >= first))
}

fn date(day: &Path) -> Option<NaiveDate> {
    let mut parts = day
        .iter()
        .rev()
        .map(|part| part.to_str()?.parse::<u32>().ok());
    let (day, month, year) = (parts.next()??, parts.next()??, parts.next()??);
    NaiveDate::from_ymd_opt(i32::try_from(year).ok()?, month, day)
}

// Cursor Agent 2026.10.22 writes `chats/<hash>/<id>/` under `CURSOR_CONFIG_DIR`,
// `$XDG_CONFIG_HOME/cursor`, or `~/.cursor`. `meta.json` records `cwd`. The first
// start passes `--new-session-id`, which reserves that directory. Start again
// passes `--resume` only while it is still there and the recorded folder is this
// thread's. A different folder belongs to another thread.
fn cursor_kept(config: &Path, conversation: Conversation, folder: &Path) -> bool {
    let name = conversation.to_string();
    let folder = dunce::canonicalize(folder).ok();
    entries(&config.join("chats")).any(|bucket| {
        let dir = bucket.join(&name);
        dir.is_dir()
            && match cursor_cwd(&dir) {
                None => true,
                Some(cwd) => folder.as_ref().is_some_and(|folder| {
                    dunce::canonicalize(&cwd).is_ok_and(|cwd| cwd == *folder)
                }),
            }
    })
}

fn cursor_cwd(dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    serde_json::from_str::<CursorMeta>(&text).ok()?.cwd
}

fn entries(folder: &Path) -> impl Iterator<Item = PathBuf> + use<> {
    std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<OsString> {
        items.iter().map(OsString::from).collect()
    }

    #[test]
    fn a_flag_picks_a_conversation_only_as_a_whole_flag() {
        assert!(picks_claude(&args(&["--resume", "abc"])));
        assert!(picks_claude(&args(&["--resume=abc"])));
        assert!(picks_claude(&args(&["-c"])));
        assert!(!picks_claude(&args(&["--resume-from", "abc"])));
        assert!(!picks_claude(&args(&["--person-arg"])));
    }

    #[test]
    fn a_codex_subcommand_picks_a_conversation() {
        assert!(picks_codex(&args(&["resume", "abc"])));
        assert!(picks_codex(&args(&["fork"])));
        assert!(!picks_codex(&args(&["--model", "x"])));
    }

    #[test]
    fn a_cursor_resume_or_new_id_picks_a_conversation() {
        assert!(picks_cursor(&args(&["--resume", "abc"])));
        assert!(picks_cursor(&args(&["--continue"])));
        assert!(picks_cursor(&args(&["--new-session-id=abc"])));
        assert!(!picks_cursor(&args(&["--plugin-dir", "x"])));
    }
}
