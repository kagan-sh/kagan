use std::path::{Path, PathBuf};

use gpui_kit::component::button::Button;
use gpui_kit::component::{ActiveTheme as _, Sizable as _, h_flex};
use gpui_kit::{ClipboardItem, Context, IntoElement as _, ParentElement as _, Styled as _, div};

use super::Kagan;
use super::row::{Group, group as row_group, row};
use crate::kagan::controls::control;
use crate::ui_registry::Facts;

// Reads the PATH a login shell sees, which a GUI app does not inherit.
pub(super) fn on_login_path() -> bool {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| "/bin/sh".into());
    std::process::Command::new(shell)
        .args(["-lc", "command -v kg"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

// The kg that ships beside kagan in the same folder.
pub(super) fn kg_binary() -> Option<PathBuf> {
    let name = if cfg!(windows) { "kg.exe" } else { "kg" };
    let path = std::env::current_exe().ok()?.parent()?.join(name);
    path.is_file().then_some(path)
}

pub(super) fn install_command(kg: &Path, macos: bool) -> String {
    if macos {
        format!(
            "sudo mkdir -p /usr/local/bin && sudo ln -sf {} /usr/local/bin/kg",
            quote(&kg.to_string_lossy())
        )
    } else {
        let folder = kg
            .parent()
            .map_or_else(String::new, |folder| folder.to_string_lossy().into_owned());
        format!("export PATH={}:\"$PATH\"", quote(&folder))
    }
}

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

pub(super) fn group(kagan: &Kagan, cx: &mut Context<Kagan>) -> Group {
    let macos = cfg!(target_os = "macos");
    let title = if macos {
        "Install the kg command"
    } else {
        "Add kg to your PATH"
    };
    let found = kagan.prefs.read(cx).kg_on_path;
    let row = match (found, kg_binary()) {
        (None, _) => row(
            "kg-command",
            title,
            "Checking your PATH…",
            div().into_any_element(),
        ),
        (Some(true), _) => row(
            "kg-command",
            title,
            "kg is on your PATH.",
            div().into_any_element(),
        ),
        (Some(false), None) => row(
            "kg-command",
            title,
            "kagan could not find kg next to itself. Reinstall kagan to get it.",
            div().into_any_element(),
        ),
        (Some(false), Some(path)) => {
            let command = install_command(&path, macos);
            let copy = command.clone();
            let body = h_flex()
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
                ));
            row(
                "kg-command",
                title,
                "Paste this into Terminal. kagan does not ask for a password.",
                body.into_any_element(),
            )
        }
    };
    row_group("kg-command", "Command line", vec![row])
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::install_command;

    #[test]
    fn the_macos_command_quotes_a_path_with_spaces_and_quotes() {
        let command = install_command(Path::new("/Applications/kagan app/kg's/kg"), true);
        assert_eq!(
            command,
            r"sudo mkdir -p /usr/local/bin && sudo ln -sf '/Applications/kagan app/kg'\''s/kg' /usr/local/bin/kg"
        );
    }

    #[test]
    fn the_other_platforms_get_a_path_line() {
        let command = install_command(Path::new("/opt/kagan/bin/kg"), false);
        assert_eq!(command, r#"export PATH='/opt/kagan/bin':"$PATH""#);
    }
}
