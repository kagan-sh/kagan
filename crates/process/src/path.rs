use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

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
    use std::os::unix::ffi::OsStringExt as _;
    let shell = std::env::var_os("SHELL")?;
    let mut command = std::process::Command::new(shell);
    command.args(["-l", "-c", "printf %s \"$PATH\""]);
    let output = crate::capture_with_limit(command, LOGIN_SHELL_TIMEOUT).ok()?;
    (output.code == 0 && !output.stdout.is_empty()).then(|| OsString::from_vec(output.stdout))
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

#[cfg(test)]
mod tests {
    use std::ffi::OsStr;
    use std::fs;
    use std::path::Path;

    use super::find_program;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("kg-path-{}-{name}", std::process::id()));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn find_program_returns_none_for_a_missing_program() {
        assert_eq!(
            find_program(
                Path::new("kg-definitely-missing"),
                Some(OsStr::new("/nonexistent")),
                None
            ),
            None
        );
    }

    #[cfg(unix)]
    #[test]
    fn find_program_finds_an_executable_on_the_path() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = temp("exe");
        let program = dir.join("kg-test-program");
        fs::write(&program, b"#!/bin/sh\n").expect("write");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            find_program(Path::new("kg-test-program"), Some(dir.as_os_str()), None),
            Some(program)
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn find_program_rejects_a_file_without_the_execute_bit() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = temp("no-exe");
        let program = dir.join("kg-test-not-executable");
        fs::write(&program, b"data").expect("write");
        fs::set_permissions(&program, fs::Permissions::from_mode(0o644)).expect("chmod");
        assert_eq!(
            find_program(
                Path::new("kg-test-not-executable"),
                Some(dir.as_os_str()),
                None
            ),
            None
        );
        fs::remove_dir_all(&dir).ok();
    }

    #[cfg(unix)]
    #[test]
    fn find_program_prefers_the_extension_when_pathext_is_set() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = temp("pathext");
        for name in ["kg-test-ext", "kg-test-ext.cmd"] {
            let file = dir.join(name);
            fs::write(&file, b"#!/bin/sh\n").expect("write");
            fs::set_permissions(&file, fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        assert_eq!(
            find_program(
                Path::new("kg-test-ext"),
                Some(dir.as_os_str()),
                Some(OsStr::new(".CMD"))
            ),
            Some(dir.join("kg-test-ext.cmd"))
        );
        fs::remove_dir_all(&dir).ok();
    }
}
