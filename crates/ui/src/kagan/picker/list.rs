use gpui_kit::component::FocusableExt as _;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::v_flex;
use gpui_kit::{
    App, Entity, Focusable as _, Hsla, InteractiveElement as _, IntoElement, MouseButton,
    ParentElement as _, StatefulInteractiveElement as _, Styled as _, div,
};
use kg_domain::{Program, split_program};

use crate::theme::TypeScale as _;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) enum Field {
    Program,
    Arguments,
}

pub(super) fn custom_command(program: &str, arguments: &str) -> Result<String, (Field, String)> {
    if program.is_empty() {
        return Err((Field::Program, "Enter a program to start.".into()));
    }
    let quoted = (program.starts_with('"') && program.ends_with('"'))
        || (program.starts_with('\'') && program.ends_with('\''));
    if program.contains(char::is_whitespace) && !quoted {
        return Err((
            Field::Program,
            "Put arguments in Arguments. Quote a program path containing spaces.".into(),
        ));
    }
    let program = if quoted {
        program.to_owned()
    } else if program.contains(['\'', '"']) {
        if !program.contains('\'') {
            format!("'{program}'")
        } else if !program.contains('"') {
            format!("\"{program}\"")
        } else {
            return Err((
                Field::Program,
                "Rename the program: it has both quote kinds.".into(),
            ));
        }
    } else {
        program.to_owned()
    };
    let command = if arguments.is_empty() {
        program
    } else {
        format!("{program} {arguments}")
    };
    split_program(&command).map_err(|_| {
        (
            Field::Arguments,
            "The arguments have an unfinished quote.".into(),
        )
    })?;
    Ok(command)
}

pub(super) fn custom_field(
    id: &'static str,
    title: &'static str,
    input: &Entity<InputState>,
    note: Option<(&'static str, String)>,
    muted: Hsla,
    cx: &App,
) -> impl IntoElement {
    let focus = input.focus_handle(cx);
    div()
        .id(id)
        .w_full()
        .ui_observe_as(Facts::input(title, input, cx))
        .on_mouse_down(MouseButton::Left, move |_, window, cx| {
            window.focus(&focus, cx);
        })
        .child(
            v_flex()
                .w_full()
                .gap_1()
                .child(div().type_small(cx).text_color(muted).child(title))
                .child(
                    Input::new(input)
                        .focus_ring(false)
                        .text_size(gpui_kit::px(crate::theme::type_body(cx))),
                )
                .children(note.map(|(id, text)| {
                    div()
                        .id(id)
                        .ui_container()
                        .type_small(cx)
                        .text_color(muted)
                        .aria_label(text.clone())
                        .child(text)
                })),
        )
}

// The number alone: the first word that starts with a digit, without a build suffix or the
// punctuation after it, so `codex-cli 0.16.0` shows `0.16.0` and `GitHub Copilot 1.0.10.` shows
// `1.0.10`. The full first line is the tooltip.
pub(super) struct Version {
    pub(super) shown: String,
    pub(super) full: Option<String>,
}

pub(super) fn version_of(program: &Program) -> Version {
    if !program.installed() {
        return Version {
            shown: "Not installed".to_owned(),
            full: None,
        };
    }
    let line = program
        .version
        .as_deref()
        .unwrap_or_default()
        .lines()
        .next()
        .unwrap_or_default()
        .trim();
    // A word such as `v2.2.5` is the number too, with its `v` left off.
    let number = line
        .split_whitespace()
        .find_map(|word| {
            let word = word
                .strip_prefix('v')
                .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
                .unwrap_or(word);
            word.starts_with(|c: char| c.is_ascii_digit())
                .then_some(word)
        })
        .unwrap_or_default()
        .split(['-', '+'])
        .next()
        .unwrap_or_default()
        .trim_end_matches(|c: char| !c.is_ascii_alphanumeric());
    Version {
        shown: number.to_owned(),
        full: (!line.is_empty()).then(|| line.to_owned()),
    }
}
