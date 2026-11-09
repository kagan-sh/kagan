use crate::kagan::dispatch;
use gpui_kit::component::switch::Switch;
use gpui_kit::{AnyElement, App, ElementId, IntoElement, Role, SharedString, Window};
use kg_domain::Op;

use crate::kagan::controls::control;
use crate::ui_registry::Facts;

pub(super) struct Row {
    pub(super) id: SharedString,
    pub(super) title: SharedString,
    pub(super) note: SharedString,
    pub(super) note_content: Option<AnyElement>,
    pub(super) control: AnyElement,
    pub(super) wide: bool,
    pub(super) alias: Option<&'static str>,
    pub(super) bare: bool,
}

pub(super) struct Group {
    pub(super) key: SharedString,
    pub(super) title: SharedString,
    pub(super) rows: Vec<Row>,
    pub(super) headed: bool,
    pub(super) style: GroupStyle,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum GroupStyle {
    #[default]
    Plain,
    Shortcuts,
    FirstShortcuts,
}

pub(super) fn row(id: &str, title: &str, note: &str, control: AnyElement) -> Row {
    Row {
        id: SharedString::from(format!("setting-row-{id}")),
        title: title.to_owned().into(),
        note: note.to_owned().into(),
        note_content: None,
        control,
        wide: false,
        alias: None,
        bare: false,
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
        style: GroupStyle::Plain,
    }
}

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
    dispatch::logged(op, cx);
}
