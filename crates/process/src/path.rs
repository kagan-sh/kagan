use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

#[cfg(unix)]
use super::process::poll_child;

/// The `PATH` of the user's login shell, which an app started from the desktop does not inherit,
/// else this process's own. Resolved once; the first call waits for the shell (at most
/// `LOGIN_SHELL_TIMEOUT`), so call [`warm_user_path`] at startup.
#[must_use]
pub fn user_path() -> Option<OsString> {
    USER_PATH
        .get_or_init(|| login_path().or_else(|| std::env::var_os("PATH")))
        .clone()
}

/// Resolves [`user_path`] on its own thread so no later caller waits on the login shell.
pub fn warm_user_path() {
    std::thread::spawn(|| {
        let _ = user_path();
    });
}

static USER_PATH: std::sync::OnceLock<Option<OsString>> = std::sync::OnceLock::new();

#[cfg(unix)]
const LOGIN_SHELL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

#[cfg(unix)]
fn login_path() -> Option<OsString> {
    use std::io::Read as _;
    use std::os::unix::ffi::OsStringExt as _;
    let shell = std::env::var_os("SHELL")?;
    let mut child = std::process::Command::new(shell)
        .args(["-l", "-c", "printf %s \"$PATH\""])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let status = poll_child(
        &mut child,
        std::process::Child::try_wait,
        |child| {
            let _ = child.kill();
            let _ = child.wait();
        },
        LOGIN_SHELL_TIMEOUT,
    )
    .ok()??;
    let mut path = Vec::new();
    child.stdout.take()?.read_to_end(&mut path).ok()?;
    (status.success() && !path.is_empty()).then(|| OsString::from_vec(path))
}

#[cfg(windows)]
fn login_path() -> Option<OsString> {
    None
}

#[must_use]
pub fn pathext() -> Option<OsString> {
    if cfg!(windows) {
        Some(std::env::var_os("PATHEXT").unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into()))
    } else {
        None
    }
}

/// Resolves `program` the way a shell would. With `pathext` (Windows), only files ending in one of
/// its extensions count, so `npx` finds `npx.cmd` and not the extensionless shell script beside it.
#[must_use]
pub fn find_program(
    program: &Path,
    path: Option<&OsStr>,
    pathext: Option<&OsStr>,
) -> Option<PathBuf> {
    let extensions: Vec<String> = pathext
        .map(|pathext| {
            pathext
                .to_string_lossy()
                .split(';')
                .filter(|extension| !extension.is_empty())
                .map(str::to_ascii_lowercase)
                .collect()
        })
        .unwrap_or_default();
    let candidates = |base: PathBuf| -> Vec<PathBuf> {
        if pathext.is_none() {
            return vec![base];
        }
        let has_extension = base.extension().is_some_and(|extension| {
            let extension = format!(".{}", extension.to_string_lossy()).to_ascii_lowercase();
            extensions.contains(&extension)
        });
        let mut with_extensions: Vec<PathBuf> = extensions
            .iter()
            .map(|extension| {
                let mut name = base.clone().into_os_string();
                name.push(extension);
                PathBuf::from(name)
            })
            .collect();
        if has_extension {
            with_extensions.insert(0, base);
        }
        with_extensions
    };

    if program.is_absolute() || program.components().count() > 1 {
        return candidates(program.to_path_buf())
            .into_iter()
            .find(|candidate| is_executable(candidate));
    }
    std::env::split_paths(path?)
        .flat_map(|dir| candidates(dir.join(program)))
        .find(|candidate| is_executable(candidate))
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

#[cfg(windows)]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}
