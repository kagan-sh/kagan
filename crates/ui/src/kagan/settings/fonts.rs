use gpui_kit::component::searchable_list::{SearchableListItem, SearchableVec};
use gpui_kit::component::select::{Select, SelectState};
use gpui_kit::{App, Entity, IntoElement, Role, SharedString, px};

use super::row::{Row, row};
use crate::kagan::controls::control;
use crate::theme::ThemeState;
use crate::ui_registry::Facts;

pub(super) fn font_row(
    id: &str,
    title: &str,
    note: &str,
    state: &Entity<FontSelect>,
    cx: &App,
) -> Row {
    let chosen = state
        .read(cx)
        .selected_value()
        .filter(|name| !name.is_empty())
        .cloned()
        .unwrap_or_else(|| "Default".to_owned());
    let picker = Select::new(state)
        .id(format!("{id}-select"))
        .menu_width(px(280.))
        .placeholder("Default")
        .accessibility_label(title.to_owned());
    let picker = control(
        id.to_owned(),
        Facts {
            role: Role::ComboBox,
            value: Some(chosen.into()),
            ..Facts::button(title)
        },
        picker,
    );
    row(id, title, note, picker.into_any_element())
}

/// One choice in a font picker: the family name, or the empty name for the theme's own font.
pub(in crate::kagan) type FontSelect = SelectState<SearchableVec<FontItem>>;

#[derive(Clone)]
pub(in crate::kagan) struct FontItem {
    value: String,
    title: SharedString,
}

impl SearchableListItem for FontItem {
    type Value = String;

    fn title(&self) -> SharedString {
        self.title.clone()
    }

    fn value(&self) -> &String {
        &self.value
    }
}

/// The choices for a font picker. The terminal's picker lists monospace faces only.
pub(super) fn font_items(cx: &App, monospace_only: bool) -> Vec<FontItem> {
    let state = ThemeState::global(cx).read(cx);
    let families = if monospace_only {
        &state.monospace_fonts
    } else {
        &state.installed_fonts
    };
    std::iter::once(FontItem {
        value: String::new(),
        title: SharedString::from("Default"),
    })
    .chain(families.iter().map(|name| FontItem {
        title: SharedString::from(name.clone()),
        value: name.clone(),
    }))
    .collect()
}
