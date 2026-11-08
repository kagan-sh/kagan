use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use kg_domain::{Environment, EnvironmentStatus, OpError};

use super::Provisioned;
use crate::host::database::Result;

pub(crate) const SCRIPT_LIMIT: Duration = Duration::from_mins(10);

const SETUP: &str = ".kagan/setup";
const TEARDOWN: &str = ".kagan/teardown";

static LOGS: AtomicU64 = AtomicU64::new(0);

enum RepoFile {
    Absent,
    Link,
    File(PathBuf),
}

pub(crate) enum SetupEnd {
    Ready(Provisioned),
    Failed(Box<(Environment, OpError)>),
    Mismatch(OpError),
}

pub(crate) fn become_ready(mut environment: Environment) -> SetupEnd {
    let notices = match confirmed(&environment) {
        Ok(notices) => notices,
        Err(Check::Mismatch) => return SetupEnd::Mismatch(mismatch_message()),
        Err(Check::Failed(error)) => {
            environment.status = EnvironmentStatus::Error;
            return SetupEnd::Failed(Box::new((environment, error)));
        }
    };
    match dunce::canonicalize(&environment.path) {
        Ok(path) => environment.path = path,
        Err(error) => {
            environment.status = EnvironmentStatus::Error;
            return SetupEnd::Failed(Box::new((
                environment,
                OpError::Failed(format!("{SETUP} could not finish: {error}")),
            )));
        }
    }
    environment.status = EnvironmentStatus::Ready;
    environment.retire_at = None;
    environment.error = None;
    SetupEnd::Ready(Provisioned {
        environment,
        notices,
    })
}

pub(super) fn teardown(root: &Path) -> Result<Vec<String>> {
    apply(root, TEARDOWN)
}

struct Object {
    mode: String,
    blob: String,
}

fn same(expected: &Object, actual: &Object) -> bool {
    if expected.blob != actual.blob {
        return false;
    }
    #[cfg(unix)]
    {
        expected.mode == actual.mode
    }
    #[cfg(not(unix))]
    {
        (expected.mode == "120000") == (actual.mode == "120000")
    }
}

enum Check {
    Mismatch,
    Failed(OpError),
}

impl From<OpError> for Check {
    fn from(error: OpError) -> Self {
        Self::Failed(error)
    }
}

fn confirmed(environment: &Environment) -> std::result::Result<Vec<String>, Check> {
    let root = &environment.path;
    let expected = expected_setup(root, environment.base_branch.as_deref())?;
    let actual = working_object(root, SETUP)?;
    if !match (&expected, &actual) {
        (Some(expected), Some(actual)) => same(expected, actual),
        (None, None) => true,
        _ => false,
    } {
        return Err(Check::Mismatch);
    }
    match expected {
        None => Ok(Vec::new()),
        Some(object) if object.mode == "120000" => {
            Ok(vec![format!("{SETUP} is a link, so kagan did not run it.")])
        }
        Some(_) => {
            run_script(root, &root.join(SETUP), SETUP, SCRIPT_LIMIT)?;
            Ok(Vec::new())
        }
    }
}

fn expected_setup(root: &Path, base: Option<&str>) -> std::result::Result<Option<Object>, Check> {
    if let Some(base) = base.filter(|base| commit_id(base)) {
        if worktree_head(root)? != base {
            return Err(Check::Mismatch);
        }
        return listed(root, base, SETUP);
    }
    listed(root, "HEAD", SETUP)
}

fn mismatch_message() -> OpError {
    OpError::Failed(format!(
        "{SETUP} changed. Start will use the version shown on screen now."
    ))
}

fn commit_id(base: &str) -> bool {
    let bytes = base.as_bytes();
    matches!(bytes.len(), 40 | 64) && bytes.iter().all(u8::is_ascii_hexdigit)
}

fn worktree_head(root: &Path) -> Result<String> {
    Ok(crate::host::git::git(root, &["rev-parse", "HEAD"])?
        .trim()
        .to_owned())
}

fn listed(root: &Path, rev: &str, name: &str) -> std::result::Result<Option<Object>, Check> {
    let text = crate::host::git::git(root, &["ls-tree", rev, "--", name])?;
    let line = text.lines().next().unwrap_or("").trim();
    if line.is_empty() {
        return Ok(None);
    }
    let mut parts = line.split_whitespace();
    let mode = parts.next().unwrap_or("");
    let kind = parts.next().unwrap_or("");
    let blob = parts.next().unwrap_or("");
    if kind != "blob" || mode.is_empty() || blob.is_empty() {
        return Err(Check::Mismatch);
    }
    Ok(Some(Object {
        mode: mode.to_owned(),
        blob: blob.to_owned(),
    }))
}

fn working_object(root: &Path, name: &str) -> Result<Option<Object>> {
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(OpError::Failed(format!(
            "{name} could not be read: {error}"
        ))),
        Ok(meta) if meta.file_type().is_symlink() => Ok(Some(Object {
            mode: "120000".to_owned(),
            blob: hash_bytes(&link_bytes(&path)?)?,
        })),
        Ok(meta) if meta.is_file() => Ok(Some(Object {
            mode: file_mode(&meta).to_owned(),
            blob: hash_file(&path)?,
        })),
        Ok(_) => Ok(None),
    }
}

fn hash_file(path: &Path) -> Result<String> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let text = crate::host::git::git(parent, &["hash-object", "--", &path.display().to_string()])?;
    Ok(text.trim().to_owned())
}

fn hash_bytes(bytes: &[u8]) -> Result<String> {
    let path = std::env::temp_dir().join(format!(
        "kg-blob-{}-{}.bin",
        std::process::id(),
        LOGS.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&path, bytes)?;
    let dir = std::env::temp_dir();
    let hashed = crate::host::git::git(&dir, &["hash-object", "--", &path.display().to_string()]);
    // The hash below is the result; the temp file is only tidied.
    let _ = std::fs::remove_file(&path);
    Ok(hashed?.trim().to_owned())
}

fn link_bytes(path: &Path) -> Result<Vec<u8>> {
    let target = std::fs::read_link(path)
        .map_err(|error| OpError::Failed(format!("{SETUP} could not be read: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt as _;
        Ok(target.as_os_str().as_bytes().to_vec())
    }
    #[cfg(not(unix))]
    {
        Ok(target.to_string_lossy().into_owned().into_bytes())
    }
}

fn file_mode(meta: &std::fs::Metadata) -> &'static str {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if meta.permissions().mode() & 0o111 != 0 {
            "100755"
        } else {
            "100644"
        }
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        "100644"
    }
}

fn apply(root: &Path, name: &str) -> Result<Vec<String>> {
    match repo_file(root, name)? {
        RepoFile::Absent => Ok(Vec::new()),
        RepoFile::Link => Ok(vec![format!("{name} is a link, so kagan did not run it.")]),
        RepoFile::File(path) => {
            run_script(root, &path, name, SCRIPT_LIMIT)?;
            Ok(Vec::new())
        }
    }
}

fn repo_file(root: &Path, name: &str) -> Result<RepoFile> {
    let path = root.join(name);
    match std::fs::symlink_metadata(&path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RepoFile::Absent),
        Err(error) => Err(OpError::Failed(format!(
            "{name} could not be read: {error}"
        ))),
        Ok(meta) if meta.file_type().is_symlink() => Ok(RepoFile::Link),
        Ok(meta) if meta.is_file() => Ok(RepoFile::File(path)),
        Ok(_) => Ok(RepoFile::Absent),
    }
}

pub(crate) fn shell_from(path: Option<&std::ffi::OsStr>, name: &str) -> Result<PathBuf> {
    kg_process::find_program(Path::new("sh"), path, kg_process::pathext().as_deref())
        .ok_or_else(|| OpError::Failed(format!("sh is not on PATH, so {name} did not run.")))
}

pub(crate) fn run_script(root: &Path, script: &Path, name: &str, limit: Duration) -> Result<()> {
    let shell = shell_from(kg_process::user_path().as_deref(), name)?;
    let log = StderrFile::create();
    let stderr = std::fs::File::create(&log.path)?;
    let mut command = std::process::Command::new(shell);
    command.current_dir(root).arg(script);
    let mut process =
        kg_process::Process::spawn(command, Stdio::null(), Stdio::null(), Stdio::from(stderr))
            .map_err(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    OpError::Failed(format!("sh is not on PATH, so {name} did not run."))
                } else {
                    OpError::Failed(format!("{name} could not start: {error}"))
                }
            })?;
    match wait_child(&mut process, limit) {
        Wait::TimedOut => Err(OpError::Failed(timeout_message(name, limit))),
        Wait::Failed(error) => Err(OpError::Failed(format!("{name} could not finish: {error}"))),
        Wait::Exited(status) if status.success() => Ok(()),
        Wait::Exited(status) => Err(script_error(name, status, &log.text())),
    }
}

enum Wait {
    Exited(std::process::ExitStatus),
    TimedOut,
    Failed(String),
}

fn wait_child(process: &mut kg_process::Process, limit: Duration) -> Wait {
    match process.wait_within(limit) {
        Ok(Some(status)) => Wait::Exited(status),
        Ok(None) => {
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if process
                    .child()
                    .try_status()
                    .is_ok_and(|status| status.is_some())
                {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Wait::TimedOut
        }
        Err(error) => Wait::Failed(error.to_string()),
    }
}

fn timeout_message(name: &str, limit: Duration) -> String {
    if limit.as_secs() == SCRIPT_LIMIT.as_secs() {
        format!("{name} ran longer than 10 minutes.")
    } else {
        format!("{name} timed out.")
    }
}

fn script_error(name: &str, status: std::process::ExitStatus, stderr: &str) -> OpError {
    let why = match status.code() {
        Some(code) => format!("{name} failed (exit {code})."),
        None => format!("{name} was stopped by a signal."),
    };
    let message = if stderr.is_empty() {
        why
    } else {
        format!("{why} {stderr}")
    };
    OpError::Failed(message)
}

struct StderrFile {
    path: PathBuf,
}

impl StderrFile {
    fn create() -> Self {
        let n = LOGS.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!("kg-script-{}-{n}.err", std::process::id()));
        Self { path }
    }

    fn text(&self) -> String {
        let Ok(mut file) = std::fs::File::open(&self.path) else {
            return String::new();
        };
        let mut bytes = [0_u8; 512];
        let read = file.read(&mut bytes).unwrap_or(0);
        let text = String::from_utf8_lossy(&bytes[..read]);
        let trimmed = text.trim();
        let mut out: String = trimmed.chars().take(200).collect();
        if trimmed.chars().nth(200).is_some() || read == bytes.len() {
            out.push('…');
        }
        out
    }
}

impl Drop for StderrFile {
    fn drop(&mut self) {
        // A temp file left behind is harmless.
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_full_sha_is_a_commit_id() {
        assert!(commit_id(&"a".repeat(40)));
        assert!(commit_id(&"b".repeat(64)));
        assert!(!commit_id(&"a".repeat(39)));
        assert!(!commit_id(&"a".repeat(41)));
        assert!(!commit_id(&"z".repeat(40)));
    }

    #[test]
    fn a_setup_file_matches_only_the_same_blob_and_mode() {
        let file = Object {
            mode: "100644".into(),
            blob: "one".into(),
        };
        assert!(same(
            &file,
            &Object {
                mode: "100644".into(),
                blob: "one".into()
            }
        ));
        assert!(!same(
            &file,
            &Object {
                mode: "100644".into(),
                blob: "two".into()
            }
        ));
        #[cfg(unix)]
        assert!(!same(
            &file,
            &Object {
                mode: "100755".into(),
                blob: "one".into()
            }
        ));
    }

    #[test]
    fn the_timeout_message_names_the_ten_minute_limit_only_for_it() {
        assert_eq!(
            timeout_message(SETUP, SCRIPT_LIMIT),
            ".kagan/setup ran longer than 10 minutes."
        );
        assert_eq!(
            timeout_message(TEARDOWN, Duration::from_secs(1)),
            ".kagan/teardown timed out."
        );
    }

    #[test]
    fn a_failed_setup_names_its_exit_code_and_stderr() {
        let status = std::process::Command::new("sh")
            .args(["-c", "exit 7"])
            .status()
            .unwrap();
        assert_eq!(
            script_error(SETUP, status, "").to_string(),
            ".kagan/setup failed (exit 7)."
        );
        let status = std::process::Command::new("sh")
            .args(["-c", "echo why >&2; exit 7"])
            .status()
            .unwrap();
        assert_eq!(
            script_error(SETUP, status, "why").to_string(),
            ".kagan/setup failed (exit 7). why"
        );
    }

    #[cfg(unix)]
    #[test]
    fn file_mode_keeps_the_executable_bit() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("setup");
        std::fs::write(&path, "").unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(file_mode(&meta), "100644");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let meta = std::fs::metadata(&path).unwrap();
        assert_eq!(file_mode(&meta), "100755");
    }
}
