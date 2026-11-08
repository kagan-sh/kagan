#[cfg(test)]
pub(crate) mod tests;
use std::future::Future;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Disableable as _, IconName, Sizable as _};
use gpui_kit::{
    Action, App, ElementId, Hsla, InteractiveElement as _, IntoElement, ParentElement as _, Pixels,
    Role, SharedString, div, px,
};
use kg_core::Workspace;
use kg_domain::{Op, OpError, OpResult, PreferenceOp};
use serde::{Serialize, de::DeserializeOwned};

use super::right;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

const WINDOW: &str = "main";

const LEFT_DEFAULT: f32 = 256.;
pub(super) const LEFT_RANGE: (f32, f32) = (200., 420.);

/// The narrowest the center may get. When the open sidebars would squeeze it below this, the
/// right sidebar is hidden first, then the left one. The window cannot be made narrower.
pub(super) const CENTER_MIN: f32 = 480.;
pub(super) const RAIL_WIDTH: f32 = 48.;

pub(super) const RIGHT_OVERLAY: &str = "RightOverlay";
pub(super) const LEFT_OVERLAY: &str = "LeftOverlay";

const LEFT_SCRIM: f32 = 0.5;

fn left_scrim(cx: &App) -> Hsla {
    cx.theme().background.opacity(LEFT_SCRIM)
}

pub(super) fn clamp_width(width: Pixels, range: (f32, f32)) -> Pixels {
    px(f32::from(width).clamp(range.0, range.1))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub(super) left_open: bool,
    pub(super) right_open: bool,
    pub(super) left_width: Pixels,
    pub(super) right_width: Pixels,
    pub(super) review_wrap: bool,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            left_open: true,
            right_open: true,
            left_width: px(LEFT_DEFAULT),
            right_width: px(right::WIDTH),
            review_wrap: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Panes {
    pub(super) left: bool,
    pub(super) right: bool,
}

/// The index of the right sidebar in the columns group. The left sidebar, when open, comes first.
pub(super) fn right_index(left_open: bool) -> usize {
    if left_open { 2 } else { 1 }
}

pub(super) fn key(name: &str) -> String {
    format!("{WINDOW}.{name}")
}

pub(crate) fn read<T: DeserializeOwned>(
    key: &str,
    cx: &mut App,
) -> impl Future<Output = Result<Option<T>, OpError>> + use<T> {
    let name = key.to_owned();
    let task = Workspace::dispatch(Op::Preference(PreferenceOp::Get { key: name.clone() }), cx);
    async move {
        match task.await? {
            OpResult::Setting(serde_json::Value::Null) => Ok(None),
            OpResult::Setting(value) => serde_json::from_value(value).map(Some).map_err(|error| {
                OpError::Failed(format!(
                    "preference {name} is not the value it was saved as: {error}"
                ))
            }),
            _ => Err(OpError::Failed(format!(
                "preference {name} did not read as a setting"
            ))),
        }
    }
}

fn saved_or<T>(read: Result<Option<T>, OpError>, fallback: T) -> T {
    read.ok().flatten().unwrap_or(fallback)
}

pub(super) fn save(key: &str, value: impl Serialize, cx: &mut App) {
    if let Ok(value) = serde_json::to_value(value) {
        Workspace::dispatch(
            Op::Preference(PreferenceOp::Set {
                key: key.into(),
                value,
            }),
            cx,
        )
        .detach();
    }
}

fn toggle(
    id: &'static str,
    name: &'static str,
    open: bool,
    disabled: bool,
    icons: (IconName, IconName),
    action: impl Action + Clone,
    cx: &App,
) -> impl IntoElement {
    let icon = if open { icons.0 } else { icons.1 };
    let button = bar_button(id, name.into(), icon, disabled, action, cx);
    let facts = Facts {
        role: Role::Button,
        name: name.into(),
        value: Some(if open { "shown" } else { "hidden" }.into()),
        disabled,
        selected: None,
        hint: None,
        focus: None,
    };
    div().id(id).occlude().ui_observe_as(facts).child(button)
}

fn bar_button(
    id: &str,
    name: SharedString,
    icon: IconName,
    disabled: bool,
    action: impl Action + Clone,
    cx: &App,
) -> Button {
    let tip = super::controls::tip(&name, &action, cx);
    Button::new(ElementId::Name(SharedString::from(format!("{id}-button"))))
        .small()
        .ghost()
        .icon(icon)
        .accessibility_label(name)
        .tooltip(tip)
        .disabled(disabled)
        .on_click(move |_, window, cx| window.dispatch_action(Box::new(action.clone()), cx))
}

mod button;
mod chrome;
mod fit;
mod layer;
mod panels;
