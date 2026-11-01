use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use kg_domain::InstallKind;

/// The copy of kagan that runs, and where it lives on disk.
#[derive(Clone, Debug)]
pub struct Target {
    pub(crate) install: InstallKind,
    /// The `.app` bundle, the AppImage file, or the running executable.
    path: PathBuf,
    /// The staged file or folder, beside `path` so the swap is a rename.
    staged: PathBuf,
    /// Where the replaced bundle waits until the next start removes it.
    old: PathBuf,
}

impl Target {
    pub(super) fn detect() -> Self {
        let exe = std::env::current_exe().unwrap_or_default();
        let appimage = std::env::var_os("APPIMAGE").map(PathBuf::from);
        let brew = std::env::var_os("HOMEBREW_PREFIX")
            .map(PathBuf::from)
            .into_iter()
            .chain([PathBuf::from("/opt/homebrew"), PathBuf::from("/usr/local")])
            .collect::<Vec<_>>();
        Self::find(std::env::consts::OS, &exe, appimage, &brew)
    }

    /// Reads how kagan was installed from where it runs. `brew` lists Homebrew prefixes.
    pub fn find(os: &str, exe: &Path, appimage: Option<PathBuf>, brew: &[PathBuf]) -> Self {
        let (install, path) = match os {
            "macos" => match bundle(exe) {
                Some(app) if cask(&app, brew) => (InstallKind::Homebrew, app),
                Some(app) => (InstallKind::App, app),
                None => (InstallKind::Other, exe.to_path_buf()),
            },
            "windows" if exe.with_file_name("uninstall.exe").is_file() => {
                (InstallKind::Installer, exe.to_path_buf())
            }
            "linux" => match appimage {
                Some(image) => (InstallKind::AppImage, image),
                None => (InstallKind::Other, exe.to_path_buf()),
            },
            _ => (InstallKind::Other, exe.to_path_buf()),
        };
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let (staged, old) = match install {
            InstallKind::Installer => (
                std::env::temp_dir().join("kagan-update-setup.exe"),
                PathBuf::new(),
            ),
            _ => (
                path.with_file_name(format!(".{name}.update")),
                path.with_file_name(format!(".{name}.old")),
            ),
        };
        Self {
            install,
            path,
            staged,
            old,
        }
    }

    /// Removes what an earlier update left beside this copy.
    pub(super) fn clean(&self) {
        if matches!(self.install, InstallKind::App | InstallKind::AppImage) {
            remove(&self.staged);
            remove(&self.old);
        }
    }

    /// Writes the checked download beside this copy and returns what `replace` moves in.
    pub(super) fn stage(&self, bytes: &[u8]) -> io::Result<PathBuf> {
        remove(&self.staged);
        match self.install {
            InstallKind::App => {
                std::fs::create_dir_all(&self.staged)?;
                unpack(bytes, &self.staged)?;
                let app = std::fs::read_dir(&self.staged)?
                    .filter_map(std::result::Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| path.extension().is_some_and(|ext| ext == "app"))
                    .ok_or_else(|| io::Error::other("the download holds no .app"))?;
                Ok(app)
            }
            InstallKind::AppImage | InstallKind::Installer => {
                std::fs::write(&self.staged, bytes)?;
                executable(&self.staged)?;
                Ok(self.staged.clone())
            }
            InstallKind::Homebrew | InstallKind::Other => Err(io::Error::other(
                "this copy of kagan is updated outside kagan",
            )),
        }
    }

    /// Moves `staged` in place of this copy. On Windows it starts the installer instead.
    pub(super) fn replace(&self, staged: &Path) -> io::Result<()> {
        if self.install == InstallKind::Installer {
            return Command::new(staged)
                .arg("/P")
                .stdin(Stdio::null())
                .spawn()
                .map(drop);
        }
        remove(&self.old);
        std::fs::rename(&self.path, &self.old)?;
        if let Err(error) = std::fs::rename(staged, &self.path) {
            std::fs::rename(&self.old, &self.path)?;
            return Err(error);
        }
        if self.install == InstallKind::App {
            remove(&self.staged);
        }
        Ok(())
    }

    /// Starts the new copy once this process has ended.
    pub(super) fn relaunch(&self) -> io::Result<()> {
        if self.install == InstallKind::Installer {
            return Command::new(&self.staged)
                .args(["/P", "/R"])
                .stdin(Stdio::null())
                .spawn()
                .map(drop);
        }
        let exe = match self.install {
            InstallKind::App => std::env::current_exe()?,
            _ => self.path.clone(),
        };
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg(r#"while kill -0 "$0" 2>/dev/null; do sleep 0.1; done; exec "$1""#)
            .arg(std::process::id().to_string())
            .arg(exe)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        command.spawn().map(drop)
    }
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

#[cfg(unix)]
fn executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn executable(_: &Path) -> io::Result<()> {
    Ok(())
}
