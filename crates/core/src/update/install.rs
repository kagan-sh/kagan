use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use kg_domain::InstallKind;

/// The copy of kagan that runs, and where it lives on disk.
#[derive(Clone, Debug, Default)]
pub struct Target {
    pub(crate) install: InstallKind,
    /// The `.app` bundle, the `AppImage` file, or the running executable.
    path: PathBuf,
    /// The staged file or folder, beside `path` so the swap is one rename.
    staged: PathBuf,
}

impl Target {
    pub(super) fn detect() -> Self {
        // `kg` on PATH is a link into the bundle.
        let exe = std::env::current_exe()
            .and_then(dunce::canonicalize)
            .unwrap_or_default();
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let appdir = std::env::var_os("APPDIR").map(PathBuf::from);
        let brew = std::env::var_os("HOMEBREW_PREFIX")
            .map(PathBuf::from)
            .into_iter()
            .chain([PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")])
            .collect::<Vec<_>>();
        Self::find(
            std::env::consts::OS,
            &exe,
            appimage,
            appdir.as_deref(),
            &brew,
        )
    }

    /// Reads how kagan was installed from where it runs. `appimage` and `appdir` are what the
    /// `AppImage` runtime set; a child process inherits them, so they count only when `exe` is
    /// inside `appdir`. `brew` lists Homebrew prefixes.
    #[must_use]
    pub fn find(
        os: &str,
        exe: &Path,
        appimage: Option<PathBuf>,
        appdir: Option<&Path>,
        brew: &[PathBuf],
    ) -> Self {
        let mounted = appdir.is_some_and(|dir| exe.starts_with(dir));
        let (install, path) = match os {
            "macos" => match bundle(exe) {
                Some(app) if cask(&app, brew) => (InstallKind::Homebrew, app),
                Some(app) => (InstallKind::App, app),
                None => (InstallKind::Other, exe.to_path_buf()),
            },
            "windows" if exe.with_file_name("uninstall.exe").is_file() => {
                (InstallKind::Installer, exe.to_path_buf())
            }
            "linux" => match appimage.filter(|_| mounted) {
                Some(image) => (InstallKind::AppImage, image),
                None => (InstallKind::Other, exe.to_path_buf()),
            },
            _ => (InstallKind::Other, exe.to_path_buf()),
        };
        let staged = if install == InstallKind::Installer {
            std::env::temp_dir().join("kagan-update-setup.exe")
        } else {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            path.with_file_name(format!(".{name}.update"))
        };
        Self {
            install,
            path,
            staged,
        }
    }

    /// Removes what an earlier update left beside this copy: a download, or the copy it replaced.
    pub(super) fn clean(&self) {
        if matches!(self.install, InstallKind::App | InstallKind::AppImage) {
            remove(&self.staged);
        }
    }

    /// Makes room for the download beside this copy, so a folder kagan cannot write fails before
    /// anything is downloaded.
    pub(super) fn prepare(&self) -> io::Result<()> {
        remove(&self.staged);
        let made = match self.install {
            InstallKind::App => std::fs::create_dir_all(&self.staged),
            InstallKind::AppImage => std::fs::File::create(&self.staged).map(drop),
            InstallKind::Installer => Ok(()),
            InstallKind::Homebrew | InstallKind::Other => {
                return Err(io::Error::other(
                    "this copy of kagan is updated outside kagan",
                ));
            }
        };
        made.map_err(|error| {
            let folder = self.path.parent().unwrap_or(&self.path).display();
            io::Error::new(
                error.kind(),
                format!(
                    "kagan cannot write {folder} ({error}). Move kagan to Applications, then update."
                ),
            )
        })
    }

    /// Writes the checked download beside this copy and returns what `replace` moves in.
    pub(super) fn stage(&self, bytes: &[u8]) -> io::Result<PathBuf> {
        match self.install {
            InstallKind::App => {
                unpack(bytes, &self.staged)?;
                std::fs::read_dir(&self.staged)?
                    .filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| path.extension().is_some_and(|ext| ext == "app"))
                    .ok_or_else(|| io::Error::other("the download holds no .app"))
            }
            InstallKind::AppImage | InstallKind::Installer => {
                std::fs::write(&self.staged, bytes)?;
                #[cfg(unix)]
                executable(&self.staged)?;
                Ok(self.staged.clone())
            }
            InstallKind::Homebrew | InstallKind::Other => Err(io::Error::other(
                "this copy of kagan is updated outside kagan",
            )),
        }
    }

    /// Puts `staged` in place of this copy in one step. The copy it replaced is left where
    /// `staged` was, for the next start to remove. On Windows it starts the installer instead.
    pub(super) fn replace(&self, staged: &Path) -> io::Result<()> {
        match self.install {
            InstallKind::App => swap(staged, &self.path),
            InstallKind::Installer => Command::new(staged)
                .arg("/P")
                .stdin(Stdio::null())
                .spawn()
                .map(drop),
            _ => std::fs::rename(staged, &self.path),
        }
    }

    /// Starts the new copy once this process has ended.
    pub(super) fn relaunch(&self) -> io::Result<()> {
        let mut command = match self.install {
            InstallKind::Installer => {
                return Command::new(&self.staged)
                    .args(["/P", "/R"])
                    .stdin(Stdio::null())
                    .spawn()
                    .map(drop);
            }
            // `open` starts the bundle as Finder would, and passes on only the `--env` it is given.
            InstallKind::App => {
                let mut command = waiting(r#"exec open -n "$@""#);
                command.arg(&self.path);
                for (name, value) in std::env::vars_os() {
                    if name.to_string_lossy().starts_with("KG_") {
                        let mut pair = name;
                        pair.push("=");
                        pair.push(value);
                        command.arg("--env").arg(pair);
                    }
                }
                command
            }
            _ => {
                let mut command = waiting(r#"exec "$@""#);
                command.arg(&self.path);
                command
            }
        };
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command.spawn().map(drop)
    }
}

// A shell that waits for this process to end, then runs `then` with the arguments added later.
fn waiting(then: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command
        .arg("-c")
        .arg(format!(
            r#"pid=$1; shift; while kill -0 "$pid" 2>/dev/null; do sleep 0.1; done; {then}"#
        ))
        .arg("sh")
        .arg(std::process::id().to_string());
    command
}

fn bundle(exe: &Path) -> Option<PathBuf> {
    let macos = exe.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.file_name()? == "MacOS"
        && contents.file_name()? == "Contents"
        && app.extension()? == "app")
        .then(|| app.to_path_buf())
}

// The cask moves the bundle into an Applications folder and keeps its receipt in the Caskroom.
fn cask(app: &Path, brew: &[PathBuf]) -> bool {
    brew.iter().any(|prefix| {
        app.starts_with(prefix)
            || (prefix.join("Caskroom/kagan").is_dir()
                && app.file_name().is_some_and(|name| name == "kagan.app")
                && app
                    .parent()
                    .and_then(Path::file_name)
                    .is_some_and(|folder| folder == "Applications"))
    })
}

fn remove(path: &Path) {
    // Best-effort cleanup: a leftover file is harmless and the install already finished.
    let _ = if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    };
}

#[cfg(target_os = "macos")]
fn unpack(bytes: &[u8], into: &Path) -> io::Result<()> {
    tar::Archive::new(flate2::read::GzDecoder::new(bytes)).unpack(into)
}

#[cfg(not(target_os = "macos"))]
fn unpack(_: &[u8], _: &Path) -> io::Result<()> {
    Err(io::Error::other("an .app update needs macOS"))
}

#[cfg(target_os = "macos")]
fn swap(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt as _;
    let from = std::ffi::CString::new(from.as_os_str().as_bytes())?;
    let to = std::ffi::CString::new(to.as_os_str().as_bytes())?;
    // SAFETY: both pointers are NUL-terminated paths that outlive the call, and renamex_np only
    // reads them.
    if unsafe { libc::renamex_np(from.as_ptr(), to.as_ptr(), libc::RENAME_SWAP) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
fn swap(_: &Path, _: &Path) -> io::Result<()> {
    Err(io::Error::other("an .app update needs macOS"))
}

#[cfg(unix)]
fn executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posix(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[test]
    fn a_mac_app_is_replaced_in_place() {
        let exe = posix("/Applications/kagan.app/Contents/MacOS/kg");
        let target = Target::find("macos", &exe, None, None, &[]);
        assert_eq!(target.install, InstallKind::App);
        assert_eq!(target.path, posix("/Applications/kagan.app"));
        assert_eq!(target.staged, posix("/Applications/.kagan.app.update"));
    }

    #[test]
    fn a_cask_app_counts_as_homebrew() {
        let exe = posix("/opt/homebrew/Caskroom/kagan/kagan.app/Contents/MacOS/kg");
        let target = Target::find("macos", &exe, None, None, &[posix("/opt/homebrew")]);
        assert_eq!(target.install, InstallKind::Homebrew);
    }

    #[test]
    fn an_executable_outside_a_bundle_is_other() {
        let exe = posix("/usr/local/bin/kg");
        let target = Target::find("macos", &exe, None, None, &[]);
        assert_eq!(target.install, InstallKind::Other);
        assert_eq!(target.path, exe);
    }

    #[test]
    fn a_windows_copy_with_an_uninstaller_is_installer() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("kg.exe");
        std::fs::write(dir.path().join("uninstall.exe"), "").unwrap();
        let target = Target::find("windows", &exe, None, None, &[]);
        assert_eq!(target.install, InstallKind::Installer);
        assert_eq!(
            target.staged,
            std::env::temp_dir().join("kagan-update-setup.exe")
        );
    }

    #[test]
    fn a_windows_copy_without_an_uninstaller_is_other() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("kg.exe");
        let target = Target::find("windows", &exe, None, None, &[]);
        assert_eq!(target.install, InstallKind::Other);
    }

    #[test]
    fn an_appimage_counts_only_while_the_runtime_mounted_it() {
        let image = posix("/home/person/kagan.AppImage");
        let mounted = posix("/tmp/.mount_kagan");
        let inside = mounted.join("usr/bin/kg");
        let target = Target::find("linux", &inside, Some(image.clone()), Some(&mounted), &[]);
        assert_eq!(target.install, InstallKind::AppImage);
        assert_eq!(target.path, image);
        assert_eq!(target.staged, posix("/home/person/.kagan.AppImage.update"));

        let outside = posix("/usr/bin/kg");
        let target = Target::find("linux", &outside, Some(image), Some(&mounted), &[]);
        assert_eq!(target.install, InstallKind::Other);
    }

    #[test]
    fn another_os_is_other() {
        let exe = posix("/opt/kagan/kg");
        let target = Target::find("freebsd", &exe, None, None, &[]);
        assert_eq!(target.install, InstallKind::Other);
        assert_eq!(target.path, exe);
    }
}
