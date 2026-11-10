use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{ClipboardItem, Context, IntoElement as _, ParentElement as _, Styled as _, div};

use super::Kagan;
use super::row::{Group, group as row_group, row};
use crate::kagan::controls::control;
use crate::theme::TypeScale as _;
use crate::ui_registry::Facts;

const CHECK_LIMIT: Duration = Duration::from_secs(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Platform {
    MacOs,
    Linux,
    Windows,
}

pub(super) fn platform() -> Platform {
    if cfg!(target_os = "macos") {
        Platform::MacOs
    } else if cfg!(windows) {
        Platform::Windows
    } else {
        Platform::Linux
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::kagan) enum PathCheck {
    OnPath,
    Missing,
    Unknown,
}

// Where kg sits next to kagan. The AppImage carries no kg, so it gets its own answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::kagan) enum Located {
    Beside(PathBuf),
    AppImage,
    Nowhere,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::kagan) struct Check {
    pub(in crate::kagan) path: PathCheck,
    pub(in crate::kagan) located: Located,
}

// Runs a program and waits at most `limit`. `None` when it could not start or ran too long.
fn within(program: &OsStr, args: &[&str], limit: Duration) -> Option<bool> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status.success()),
            Ok(None) if started.elapsed() < limit => std::thread::sleep(Duration::from_millis(20)),
            Ok(None) => {
                _ = child.kill();
                _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
}

// Asks the login shell, which sees the PATH a GUI app does not inherit.
fn path_check(platform: Platform) -> PathCheck {
    match platform {
        Platform::Windows => {
            let on_path = std::env::var_os("PATH").is_some_and(|path| {
                std::env::split_paths(&path).any(|dir| dir.join("kg.exe").is_file())
            });
            if on_path {
                PathCheck::OnPath
            } else {
                PathCheck::Missing
            }
        }
        Platform::MacOs | Platform::Linux => {
            let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
            match within(&shell, &["-lc", "command -v kg"], CHECK_LIMIT) {
                Some(true) => PathCheck::OnPath,
                Some(false) => PathCheck::Missing,
                None => PathCheck::Unknown,
            }
        }
    }
}

// Pure: the folder kagan runs from, and whether kg is in it.
pub(super) fn locate(
    folder: Option<&Path>,
    appimage: bool,
    platform: Platform,
    is_file: impl Fn(&Path) -> bool,
) -> Located {
    if appimage {
        return Located::AppImage;
    }
    let name = if platform == Platform::Windows {
        "kg.exe"
    } else {
        "kg"
    };
    match folder.map(|folder| folder.join(name)) {
        Some(path) if is_file(&path) => Located::Beside(path),
        _ => Located::Nowhere,
    }
}

pub(super) fn probe(appimage: bool) -> Check {
    let platform = platform();
    let folder = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(Path::to_path_buf));
    Check {
        path: path_check(platform),
        located: locate(folder.as_deref(), appimage, platform, Path::is_file),
    }
}

// The line the person runs, or pastes into a file, for a kg that is not on the PATH.
pub(super) fn install_command(kg: &Path, platform: Platform) -> String {
    let folder = kg
        .parent()
        .map_or_else(String::new, |folder| folder.to_string_lossy().into_owned());
    match platform {
        Platform::MacOs => format!(
            "sudo mkdir -p /usr/local/bin && sudo ln -sf {} /usr/local/bin/kg",
            quote(&kg.to_string_lossy())
        ),
        Platform::Linux => format!("export PATH={}:\"$PATH\"", quote(&folder)),
        Platform::Windows => folder,
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

// What the row shows. Pure, so every state can be tested on any platform.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Shown {
    Checking,
    OnPath,
    CouldNotCheck,
    Command(String),
    AppImage,
    Nowhere,
}

pub(super) fn shown(check: Option<&Check>, platform: Platform) -> Shown {
    let Some(check) = check else {
        return Shown::Checking;
    };
    match (check.path, &check.located) {
        (PathCheck::OnPath, _) => Shown::OnPath,
        (PathCheck::Unknown, _) => Shown::CouldNotCheck,
        (PathCheck::Missing, Located::Beside(kg)) => Shown::Command(install_command(kg, platform)),
        (PathCheck::Missing, Located::AppImage) => Shown::AppImage,
        (PathCheck::Missing, Located::Nowhere) => Shown::Nowhere,
    }
}

pub(super) fn group(kagan: &Kagan, cx: &mut Context<Kagan>) -> Group {
    let platform = platform();
    let title = match platform {
        Platform::MacOs => "Install the kg command",
        Platform::Linux | Platform::Windows => "Add kg to your PATH",
    };
    let check = kagan.prefs.read(cx).kg_check.clone();
    let body = match shown(check.as_ref(), platform) {
        Shown::Checking => plain("Checking your PATH…", cx),
        Shown::OnPath => plain("kg is on your PATH.", cx),
        Shown::CouldNotCheck => plain(
            "Could not check your PATH. Open a new terminal and reopen kagan to try again.",
            cx,
        ),
        Shown::Nowhere => plain(
            "kagan could not find kg next to itself. Reinstall kagan to get it.",
            cx,
        ),
        Shown::AppImage => plain(
            "The AppImage does not include kg. Install the .deb or build from source to get it.",
            cx,
        ),
        Shown::Command(command) => command_row(command, platform, cx),
    };
    row_group(
        "kg-command",
        "Command line",
        vec![row("kg-command", title, "", body)],
    )
}

fn plain(text: &str, cx: &mut Context<Kagan>) -> gpui_kit::AnyElement {
    div()
        .type_small(cx)
        .text_color(cx.theme().muted_foreground)
        .child(text.to_owned())
        .into_any_element()
}

fn command_row(
    command: String,
    platform: Platform,
    cx: &mut Context<Kagan>,
) -> gpui_kit::AnyElement {
    let copy = command.clone();
    let note = match platform {
        Platform::MacOs => "Paste this into Terminal. kagan does not ask for a password.",
        Platform::Linux => "Paste this into your shell profile, then open a new terminal.",
        Platform::Windows => "Add this folder to your Path, then open a new terminal.",
    };
    v_stack(note, cx)
        .child(
            h_flex()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .font_family(cx.theme().mono_font_family.clone())
                        .child(command),
                )
                .child(control(
                    "kg-command-copy",
                    Facts::button("Copy"),
                    Button::new("kg-command-copy-button")
                        .small()
                        .outline()
                        .label("Copy")
                        .on_click(cx.listener(move |_, _, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copy.clone()));
                        })),
                )),
        )
        .into_any_element()
}

fn v_stack(note: &str, cx: &mut Context<Kagan>) -> gpui_kit::Div {
    gpui_kit::component::v_flex().gap_2().child(
        div()
            .type_small(cx)
            .text_color(cx.theme().muted_foreground)
            .child(note.to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::time::Duration;

    use super::{Check, Located, PathCheck, Platform, install_command, locate, shown, within};

    #[test]
    fn the_macos_command_quotes_a_path_with_spaces_and_quotes() {
        let command = install_command(
            Path::new("/Applications/kagan app/kg's/kg"),
            Platform::MacOs,
        );
        assert_eq!(
            command,
            r"sudo mkdir -p /usr/local/bin && sudo ln -sf '/Applications/kagan app/kg'\''s/kg' /usr/local/bin/kg"
        );
    }

    #[test]
    fn linux_gets_a_profile_line_and_windows_gets_the_folder() {
        let kg = Path::new("/opt/kagan/bin/kg");
        assert_eq!(
            install_command(kg, Platform::Linux),
            r#"export PATH='/opt/kagan/bin':"$PATH""#
        );
        assert_eq!(install_command(kg, Platform::Windows), "/opt/kagan/bin");
    }

    #[test]
    fn every_state_has_one_row() {
        let beside = Located::Beside("/opt/kagan/bin/kg".into());
        let check = |path, located| Check { path, located };
        assert_eq!(shown(None, Platform::Linux), super::Shown::Checking);
        assert_eq!(
            shown(
                Some(&check(PathCheck::OnPath, beside.clone())),
                Platform::Linux
            ),
            super::Shown::OnPath
        );
        assert_eq!(
            shown(
                Some(&check(PathCheck::Unknown, beside.clone())),
                Platform::Linux
            ),
            super::Shown::CouldNotCheck
        );
        assert_eq!(
            shown(
                Some(&check(PathCheck::Missing, Located::AppImage)),
                Platform::Linux
            ),
            super::Shown::AppImage
        );
        assert_eq!(
            shown(
                Some(&check(PathCheck::Missing, Located::Nowhere)),
                Platform::Linux
            ),
            super::Shown::Nowhere
        );
        assert_eq!(
            shown(Some(&check(PathCheck::Missing, beside)), Platform::Windows),
            super::Shown::Command("/opt/kagan/bin".into())
        );
    }

    #[test]
    fn kg_is_found_beside_kagan_unless_this_is_an_appimage() {
        let folder = Path::new("/apps/kagan");
        let present = |path: &Path| path == Path::new("/apps/kagan/kg");
        assert_eq!(
            locate(Some(folder), false, Platform::Linux, present),
            Located::Beside("/apps/kagan/kg".into())
        );
        assert_eq!(
            locate(Some(folder), true, Platform::Linux, present),
            Located::AppImage
        );
        assert_eq!(
            locate(Some(folder), false, Platform::Windows, present),
            Located::Nowhere
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_check_that_runs_too_long_gives_up() {
        let started = std::time::Instant::now();
        assert_eq!(
            within(
                "sh".as_ref(),
                &["-c", "sleep 5"],
                Duration::from_millis(100)
            ),
            None
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[cfg(unix)]
    #[test]
    fn a_check_that_finishes_reports_its_result() {
        let limit = Duration::from_secs(3);
        assert_eq!(within("sh".as_ref(), &["-c", "exit 0"], limit), Some(true));
        assert_eq!(within("sh".as_ref(), &["-c", "exit 1"], limit), Some(false));
    }
}
