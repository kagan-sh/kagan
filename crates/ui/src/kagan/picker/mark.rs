use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div,
    px, svg,
};
use kg_domain::{Program, ProgramSource, split_program};

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;

// The command name of each agent with a bundled mark, and the mark's path in `brand.rs`.
const AGENT_MARKS: &[(&str, &str)] = &[
    ("claude", "agents/claude.svg"),
    ("codex", "agents/codex.svg"),
    ("gemini", "agents/gemini.svg"),
    ("copilot", "agents/copilot.svg"),
    ("opencode", "agents/opencode.svg"),
];

const MARK: f32 = 20.;

// The one mark for a program: a shell gets the terminal, a known agent its mark, anything else
// its first letter. Every mark takes one tint, so it reads in light and dark alike. An icon and a
// letter have different ids, so a screen shows which one a program has.
pub(crate) fn program_mark(program: &Program, cx: &App) -> AnyElement {
    let color = cx.theme().muted_foreground;
    let label = program.label.clone();
    let frame = div()
        .flex_shrink_0()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center();
    if program.source == ProgramSource::Shell {
        return frame
            .id(format!("program-icon-{label}"))
            .ui_container()
            .child(Icon::new(IconName::SquareTerminal).text_color(color))
            .into_any_element();
    }
    if let Some(path) = agent_path(&program.command) {
        return frame
            .id(format!("program-icon-{label}"))
            .ui_container()
            .child(svg().path(path).size(px(MARK)).text_color(color))
            .into_any_element();
    }
    let letter = label
        .chars()
        .next()
        .map(|letter| letter.to_uppercase().to_string())
        .unwrap_or_default();
    frame
        .id(format!("program-mark-{label}"))
        .ui_container()
        .rounded(cx.theme().radius)
        .bg(cx.theme().muted)
        .type_small(cx)
        .text_color(cx.theme().foreground)
        .child(letter)
        .into_any_element()
}

fn agent_path(command: &str) -> Option<&'static str> {
    let (name, _) = split_program(command).ok()?;
    let name = std::path::Path::new(&name).file_stem()?.to_str()?;
    AGENT_MARKS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, path)| *path)
}

#[cfg(test)]
mod tests {
    use super::agent_path;

    #[test]
    fn a_known_command_maps_to_its_mark_with_or_without_arguments_or_a_path() {
        assert_eq!(agent_path("claude"), Some("agents/claude.svg"));
        assert_eq!(agent_path("claude --resume"), Some("agents/claude.svg"));
        assert_eq!(agent_path("/usr/local/bin/codex"), Some("agents/codex.svg"));
    }

    #[test]
    fn an_unknown_command_has_no_mark() {
        assert_eq!(agent_path("aider"), None);
        assert_eq!(agent_path("my-tool --flag"), None);
    }
}
