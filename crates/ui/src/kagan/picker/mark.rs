use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::{Icon, IconName};
use gpui_kit::{
    AnyElement, App, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div,
    px, svg,
};
use kg_domain::{AGENT_MARKS, Program, ProgramSource, agent_name};

use crate::kagan::controls::program_key;
use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;

const MARK: f32 = 20.;

// The one mark for a program: a shell gets the terminal, a known agent its mark, anything else
// its first letter. Every mark takes one tint, so it reads in light and dark alike. An icon and a
// letter have different ids, so a screen shows which one a program has.
pub(crate) fn program_mark(program: &Program, cx: &App) -> AnyElement {
    let color = cx.theme().muted_foreground;
    let label = program.label.clone();
    let key = program_key(program);
    let frame = div()
        .flex_shrink_0()
        .size(px(MARK))
        .flex()
        .items_center()
        .justify_center();
    if program.source == ProgramSource::Shell {
        return frame
            .id(format!("program-icon-{key}"))
            .ui_container()
            .child(Icon::new(IconName::SquareTerminal).text_color(color))
            .into_any_element();
    }
    // A saved program keeps its letter, as main decided: its name is its own, not the agent's.
    if let Some(path) =
        agent_path(&program.command).filter(|_| program.source != ProgramSource::Saved)
    {
        return frame
            .id(format!("program-icon-{key}"))
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
        .id(format!("program-mark-{key}"))
        .ui_container()
        .rounded(cx.theme().radius)
        .bg(cx.theme().muted)
        .type_small(cx)
        .text_color(cx.theme().foreground)
        .child(letter)
        .into_any_element()
}

fn agent_path(command: &str) -> Option<&'static str> {
    command_mark(agent_name(command)?)
}

/// The bundled mark of the agent whose command name is `name`, for any surface that shows an
/// agent, such as the picker or the usage popup.
pub(crate) fn command_mark(name: &str) -> Option<&'static str> {
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
        assert_eq!(agent_path("claude --resume"), Some("agents/claude.svg"));
        assert_eq!(
            agent_path("/usr/local/bin/cursor-agent"),
            Some("agents/cursor.svg")
        );
        assert_eq!(agent_path("aider"), None);
    }

    #[test]
    fn every_mark_path_is_bundled() {
        use gpui_kit::AssetSource as _;
        for (command, path) in kg_domain::AGENT_MARKS {
            let bundled = crate::brand::Assets.load(path).ok().flatten();
            assert!(
                bundled.is_some(),
                "the mark for {command} is not bundled: {path}"
            );
        }
    }
}
