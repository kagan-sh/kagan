//! A plugin's settings page: its declared settings as rows, then its own view when it has one.
//! A save reply is applied only when that save is still the latest and the plugin activation is still the one captured when the save started.

use crate::theme::TypeScale as _;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{Sizable as _, h_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, PathPromptOptions, SharedString,
    StatefulInteractiveElement as _, Styled as _, WeakEntity, Window, div, rems,
};
use kg_core::Workspace;
use kg_domain::PluginId;
use serde_json::{Value, json};

use super::{Change, Secret, save_value};
use crate::kagan::Kagan;
use crate::kagan::controls::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use gpui_kit::Role;

pub(super) fn secret_field(
    scope: &str,
    plugin: PluginId,
    key: String,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Secret {
    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .masked(true)
            .placeholder("Paste a new value")
    });
    let scope = scope.to_owned();
    let enter = cx.subscribe_in(&input, window, move |kagan, input, event, window, cx| {
        if let InputEvent::PressEnter { .. } = event
            && let Some(text) = take_secret(input, window, cx)
        {
            let (scope, plugin, key) = (scope.clone(), plugin.clone(), key.clone());
            kagan
                .settings_drafts
                .begin(scope, plugin, key, text, None, cx);
        }
    });
    Secret {
        input,
        _enter: enter,
    }
}

pub(super) fn take_secret(
    input: &Entity<InputState>,
    window: &mut Window,
    cx: &mut App,
) -> Option<Change> {
    let text = input.read(cx).value().to_string();
    input.update(cx, |state, cx| state.set_value("", window, cx));
    (!text.is_empty()).then(|| Change::Set(json!(text)))
}

pub(super) fn secret_control(
    weak: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    title: &str,
    input: &Entity<InputState>,
    set: bool,
    cx: &mut Context<Kagan>,
) -> AnyElement {
    let scope = &format!("{}/{key}", id.0);
    let label = if set { "Replace" } else { "Set" };
    let save = {
        let (weak, scope, plugin, key, input) = (
            weak.clone(),
            scope.to_owned(),
            id.clone(),
            key.to_owned(),
            input.clone(),
        );
        move |_: &gpui_kit::ClickEvent, window: &mut Window, cx: &mut App| {
            if let Some(text) = take_secret(&input, window, cx) {
                save_value(&weak, scope.clone(), plugin.clone(), key.clone(), text, cx);
            }
        }
    };
    let clear = set.then(|| {
        let (weak, scope, plugin, key) =
            (weak.clone(), scope.to_owned(), id.clone(), key.to_owned());
        control(
            format!("setting-clear-{scope}"),
            Facts::button("Clear"),
            Button::new(ElementId::Name(
                format!("button-setting-clear-{scope}").into(),
            ))
            .small()
            .outline()
            .label("Clear")
            .on_click(move |_, _, cx| {
                let (scope, plugin, key) = (scope.clone(), plugin.clone(), key.clone());
                save_value(&weak, scope, plugin, key, Change::Clear, cx);
            }),
        )
    });
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .type_small(cx)
                .child(if set { "Set" } else { "Not set" }),
        )
        .child(
            div()
                .id(ElementId::Name(format!("plugin-secret-{scope}").into()))
                .ui_observe_as(Facts::input(title, input, cx))
                .w(rems(13.75))
                .h(rems(1.75))
                .child(
                    Input::new(input)
                        .text_size(gpui_kit::px(crate::theme::type_body(cx)))
                        .small(),
                ),
        )
        .child(control(
            format!("setting-secret-{scope}"),
            Facts::button(label),
            Button::new(ElementId::Name(
                format!("button-setting-secret-{scope}").into(),
            ))
            .small()
            .outline()
            .label(label)
            .on_click(save),
        ))
        .children(clear)
        .into_any_element()
}

pub(super) fn project_control(
    weak: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    scope: &str,
    value: &Value,
    cx: &App,
) -> (String, AnyElement) {
    let current = value.as_str().unwrap_or_default().to_owned();
    let projects: Vec<(String, String)> = Workspace::global(cx)
        .read(cx)
        .sidebar(cx)
        .projects
        .into_iter()
        .map(|project| (project.id.0, project.name))
        .collect();
    let (label, tip, note) = if current.is_empty() {
        ("None".to_owned(), None, String::new())
    } else if let Some((_, name)) = projects.iter().find(|(project, _)| project == &current) {
        (short_label(name, 28), Some(name.clone()), String::new())
    } else {
        (
            "Missing".to_owned(),
            Some(current.clone()),
            "That project is gone. Pick another or None.".into(),
        )
    };
    let (plugin, setting_key, scope) = (id.clone(), key.to_owned(), scope.to_owned());
    let weak = weak.clone();
    let projects_for_menu = projects;
    let mut button = Button::new(ElementId::Name(
        format!("button-setting-project-{scope}").into(),
    ))
    .small()
    .outline()
    .label(label.clone());
    if let Some(tip) = tip {
        button = button.tooltip(tip);
    }
    let control = control(
        format!("setting-project-{scope}"),
        Facts::button(label),
        button.dropdown_menu(move |menu, _, _| {
            project_menu(
                menu,
                &weak,
                &plugin,
                &setting_key,
                &scope,
                &current,
                &projects_for_menu,
            )
        }),
    )
    .into_any_element();
    (note, control)
}

pub(super) fn short_label(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let mut end = 0;
    let mut last_break = 0;
    for (index, ch) in text.char_indices() {
        if text[..index].chars().count() >= max {
            break;
        }
        end = index + ch.len_utf8();
        if ch.is_whitespace() || matches!(ch, '-' | '_' | '/' | '.') {
            last_break = end;
        }
    }
    let cut = if last_break > max / 2 {
        last_break
    } else {
        end
    };
    format!("{}…", text[..cut].trim_end())
}

pub(super) fn project_menu(
    menu: PopupMenu,
    weak: &WeakEntity<Kagan>,
    plugin: &PluginId,
    key: &str,
    scope: &str,
    current: &str,
    projects: &[(String, String)],
) -> PopupMenu {
    let none_id = ElementId::Name(format!("project-none-{scope}").into());
    let none_scope = scope.to_owned();
    let none_plugin = plugin.clone();
    let none_key = key.to_owned();
    let none_weak = weak.clone();
    let none_selected = current.is_empty();
    let menu = menu.scrollable(true).item(
        PopupMenuItem::element(move |_, _| {
            div()
                .id(none_id.clone())
                .ui_observe_as(Facts {
                    role: Role::MenuItem,
                    selected: Some(none_selected),
                    ..Facts::button("None")
                })
                .child("None")
        })
        .checked(none_selected)
        .on_click(move |_, _, cx| {
            save_value(
                &none_weak,
                none_scope.clone(),
                none_plugin.clone(),
                none_key.clone(),
                Change::Clear,
                cx,
            );
        }),
    );
    projects.iter().fold(menu, |menu, (project_id, name)| {
        let (weak, scope, plugin, key, pick, full, selected) = (
            weak.clone(),
            scope.to_owned(),
            plugin.clone(),
            key.to_owned(),
            project_id.clone(),
            name.clone(),
            current == project_id,
        );
        let shown = short_label(&full, 40);
        let item_id = format!("project-pick-{scope}/{pick}");
        menu.item(
            PopupMenuItem::element(move |_, _| {
                div()
                    .id(ElementId::Name(item_id.clone().into()))
                    .tooltip({
                        let text = full.clone();
                        move |window, cx| Tooltip::new(text.clone()).build(window, cx)
                    })
                    .ui_observe_as(Facts {
                        role: Role::MenuItem,
                        selected: Some(selected),
                        hint: Some(full.clone().into()),
                        ..Facts::button(full.clone())
                    })
                    .child(shown.clone())
            })
            .checked(selected)
            .on_click(move |_, _, cx| {
                save_value(
                    &weak,
                    scope.clone(),
                    plugin.clone(),
                    key.clone(),
                    Change::Set(json!(pick.clone())),
                    cx,
                );
            }),
        )
    })
}

pub(super) fn select_control(
    weak: &WeakEntity<Kagan>,
    id: &PluginId,
    key: &str,
    scope: &str,
    options: &[String],
    value: &Value,
) -> AnyElement {
    let current = value.as_str().unwrap_or_default().to_owned();
    h_flex()
        .w_full()
        .min_w_0()
        .gap_1()
        .flex_wrap()
        .justify_end()
        .children(options.iter().map(|option| {
            let option_id = format!("option-{scope}/{option}");
            let (plugin, setting_key, pick, scope, weak) = (
                id.clone(),
                key.to_owned(),
                option.clone(),
                scope.to_owned(),
                weak.clone(),
            );
            let button = Button::new(ElementId::Name(
                format!("button-option-{scope}/{option}").into(),
            ))
            .small()
            .label(option.clone())
            .on_click(move |_, _, cx| {
                save_value(
                    &weak,
                    scope.clone(),
                    plugin.clone(),
                    setting_key.clone(),
                    Change::Set(json!(pick)),
                    cx,
                );
            });
            let button = if *option == current {
                button.primary()
            } else {
                button.outline()
            };
            div().flex_shrink_0().child(control(
                option_id,
                Facts {
                    role: Role::RadioButton,
                    selected: Some(*option == current),
                    ..Facts::button(option.clone())
                },
                button,
            ))
        }))
        .into_any_element()
}

pub(super) fn folder_control(
    weak: &WeakEntity<Kagan>,
    scope: &str,
    id: &PluginId,
    key: &str,
    shown: String,
    cx: &App,
) -> AnyElement {
    let (plugin, setting, scope) = (id.clone(), key.to_owned(), scope.to_owned());
    let weak = weak.clone();
    h_flex()
        .gap_2()
        .child(div().type_small(cx).child(SharedString::from(shown)))
        .child(control(
            format!("choose-{}/{key}", id.0),
            Facts::button("Choose…"),
            Button::new(ElementId::Name(
                format!("button-choose-{}/{key}", id.0).into(),
            ))
            .small()
            .outline()
            .label("Choose…")
            .on_click(move |_, _, cx| {
                choose_folder(
                    weak.clone(),
                    scope.clone(),
                    plugin.clone(),
                    setting.clone(),
                    cx,
                );
            }),
        ))
        .into_any_element()
}

pub(super) fn choose_folder(
    weak: WeakEntity<Kagan>,
    scope: String,
    id: PluginId,
    key: String,
    cx: &mut App,
) {
    let picked = cx.prompt_for_paths(PathPromptOptions {
        files: false,
        directories: true,
        multiple: false,
        prompt: Some("Choose".into()),
    });
    cx.spawn(async move |cx| {
        let Ok(Ok(Some(paths))) = picked.await else {
            return;
        };
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        cx.update(|cx| {
            save_value(
                &weak,
                scope,
                id,
                key,
                Change::Set(json!(path.to_string_lossy().into_owned())),
                cx,
            );
        });
    })
    .detach();
}
