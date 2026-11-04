//! The row and group builders every settings page uses.

use gpui_kit::component::switch::Switch;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, Role, SharedString, Window};
use kg_core::Workspace;
use kg_domain::Op;

use crate::kagan::shown::control;
use crate::ui_registry::Facts;

/// One row: a title, a muted line, and its control on the right.
pub(super) struct Row {
    pub(super) id: SharedString,
    pub(super) title: SharedString,
    pub(super) note: SharedString,
    pub(super) note_content: Option<AnyElement>,
    pub(super) control: AnyElement,
    /// A wide control stacks under the title
    pub(super) wide: bool,
}

/// A titled card of rows.
pub(super) struct Group {
    /// Names the group's container, so it can be found. Plugin groups are keyed by plugin id.
    pub(super) key: SharedString,
    pub(super) title: SharedString,
    pub(super) rows: Vec<Row>,
    pub(super) headed: bool,
}

/// One row, with its id, its title, its muted line, and its control.
pub(super) fn row(id: &str, title: &str, note: &str, control: AnyElement) -> Row {
    Row {
        id: SharedString::from(format!("setting-row-{id}")),
        title: title.to_owned().into(),
        note: note.to_owned().into(),
        note_content: None,
        control,
        wide: false,
    }
}

pub(super) fn group(
    key: impl Into<SharedString>,
    title: impl Into<SharedString>,
    rows: Vec<Row>,
) -> Group {
    Group {
        key: key.into(),
        title: title.into(),
        rows,
        headed: false,
    }
}

/// A switch that reports its new value. It takes the theme's colors.
pub(super) fn switch(
    id: &str,
    title: &str,
    on: bool,
    set: impl Fn(bool, &mut Window, &mut App) + 'static,
) -> AnyElement {
    control(
        id.to_owned(),
        Facts {
            role: Role::Switch,
            value: Some(if on { "on" } else { "off" }.into()),
            ..Facts::button(title)
        },
        Switch::new(ElementId::Name(format!("{id}-switch").into()))
            .checked(on)
            .on_click(move |checked: &bool, window, cx: &mut App| set(*checked, window, cx)),
    )
    .into_any_element()
}

pub(super) fn save(op: Op, cx: &mut App) {
    Workspace::dispatch(op, cx).detach();
}
