use crate::theme::TypeScale as _;

use gpui_kit::component::ThemeMode;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, Context, ElementId, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, rems,
};
use kg_domain::{Appearance, Op, PluginId, PreferenceOp, Slot, ThemeFile, ThemeOp};
use kg_plugins::{PluginHost, Setting};
use serde_json::json;

use super::indexes;
use super::installed;
use super::nav::Category;
use super::plugins;
use super::row::{Group, Row, group, row, save, switch};
use super::{ActivateItem, ITEM, Kagan};
use crate::kagan::controls::control;
use crate::theme::{CatalogEntry, Source, ThemeState, swatches};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn groups(kagan: &Kagan, window: &mut Window, cx: &mut Context<Kagan>) -> Vec<Group> {
    let prefs = kagan.prefs.read(cx);
    let (agent, notify) = (prefs.agent_integration, prefs.notify);
    let (confirm_archive, plugin_status) = (prefs.confirm_archive, prefs.plugin_status);
    let mut appearance = theme_group(window, cx);
    appearance.key = "appearance".into();
    appearance.title = "Appearance".into();
    appearance.rows.extend(mode_group(cx).rows);
    appearance.rows.push(terminal_size(kagan));
    let mut out = vec![
        within(general(agent, notify, confirm_archive), Category::General),
        within(environment(kagan), Category::General),
        within(appearance, Category::Appearance),
    ];
    out.extend(
        super::keyboard::groups(kagan, window, cx)
            .into_iter()
            .map(|group| within(group, Category::Keyboard)),
    );
    let mut installed = installed::group(kagan, window, cx);
    installed.title = "Installed".into();
    let slots = Slot::ALL.map(|slot| super::slots::row(kagan, slot, cx));
    out.push(within(installed, Category::Installed));
    out.push(within(indexes::group(kagan, cx), Category::Installed));
    out.push(within(
        group(
            "activity",
            "Activity",
            vec![plugin_status_row(plugin_status)],
        ),
        Category::Installed,
    ));
    out.push(within(
        group("slots", "Replace parts", slots.into()),
        Category::Installed,
    ));
    out.push(within(super::updates::group(kagan, cx), Category::Updates));
    for (id, _) in plugin_entries(cx) {
        for group in plugins::groups(kagan, &id, cx) {
            out.push(within(group, Category::Plugin(id.clone())));
        }
    }
    headed(out)
}

fn set_flag(key: &str, on: bool, cx: &mut App) {
    save(
        Op::Preference(PreferenceOp::Set {
            key: key.into(),
            value: json!(on),
        }),
        cx,
    );
}

fn within(mut group: Group, section: Category) -> Group {
    group.section = section;
    group
}

// A section with one group needs no heading: the section name already says it.
fn headed(mut out: Vec<Group>) -> Vec<Group> {
    for index in 0..out.len() {
        let section = out[index].section.clone();
        let groups = out.iter().filter(|group| group.section == section).count();
        out[index].headed = groups > 1;
    }
    out
}

pub(super) fn plugin_entries(cx: &App) -> Vec<(PluginId, String)> {
    let Some(host) = PluginHost::try_global(cx) else {
        return Vec::new();
    };
    let views: Vec<PluginId> = kg_plugins::contributions(cx)
        .map(|list| {
            list.into_iter()
                .filter(|(_, ui)| ui.settings.as_deref().is_some())
                .map(|(id, _)| id)
                .collect()
        })
        .unwrap_or_default();
    let host = host.read(cx);
    host.status()
        .iter()
        .filter(|status| status.enabled)
        .filter(|status| {
            views.contains(&status.id)
                || host
                    .settings(&status.id)
                    .iter()
                    .any(|(_, setting, _)| !matches!(setting, Setting::Provider { .. }))
        })
        .map(|status| (status.id.clone(), status.name.as_str().to_owned()))
        .collect()
}

fn general(agent: bool, notify: bool, confirm_archive: bool) -> Group {
    group(
        "agents",
        "Agents",
        vec![
            row(
                "agent-integration",
                "Agent integration",
                "Off keeps kagan’s tools out of the agents it starts.",
                switch(
                    "agent-integration",
                    "Agent integration",
                    agent,
                    |on, _, cx| {
                        set_flag(kg_domain::prefs::AGENT_INTEGRATION, on, cx);
                    },
                ),
            ),
            row(
                "notify",
                "Notifications",
                "A system notification when a thread needs you and is not on screen.",
                switch("notify", "Notifications", notify, |on, _, cx| {
                    set_flag(kg_domain::prefs::NOTIFY, on, cx);
                }),
            ),
            row(
                "confirm-archive",
                "Ask before archiving",
                "Ask before a thread moves to Archived.",
                switch(
                    "confirm-archive",
                    "Ask before archiving",
                    confirm_archive,
                    |on, _, cx| {
                        set_flag(kg_domain::prefs::CONFIRM_ARCHIVE, on, cx);
                    },
                ),
            ),
        ],
    )
}

fn environment(kagan: &Kagan) -> Group {
    group(
        "environment",
        "Environment variables",
        vec![row(
            "environment-variables",
            "Variables",
            "Set once for every program kagan starts. A value is hidden after you set it.",
            kagan.settings.variables.clone().into_any_element(),
        )],
    )
}

fn plugin_status_row(on: bool) -> Row {
    row(
        "plugin-status",
        "Plugin status",
        "Show items that plugins add to the activity strip. Off hides them all; each plugin can still be turned off in Installed.",
        switch("plugin-status", "Plugin status", on, |on, _, cx| {
            set_flag(kg_domain::prefs::PLUGIN_STATUS, on, cx);
        }),
    )
}

fn terminal_size(kagan: &Kagan) -> Row {
    let size = kagan.terminal_font_size;
    let buttons = h_flex()
        .gap_1()
        .child(control(
            "terminal-size-smaller",
            Facts {
                disabled: size <= kg_domain::prefs::TERMINAL_FONT_MIN,
                ..Facts::button("Smaller terminal text")
            },
            Button::new("button-terminal-size-smaller")
                .small()
                .outline()
                .label("−")
                .accessibility_label("Smaller terminal text")
                .disabled(size <= kg_domain::prefs::TERMINAL_FONT_MIN)
                .on_click(super::super::controls::dispatch(super::super::Smaller)),
        ))
        .child(div().child(format!("{size} pt")))
        .child(control(
            "terminal-size-bigger",
            Facts {
                disabled: size >= kg_domain::prefs::TERMINAL_FONT_MAX,
                ..Facts::button("Bigger terminal text")
            },
            Button::new("button-terminal-size-bigger")
                .small()
                .outline()
                .label("+")
                .accessibility_label("Bigger terminal text")
                .disabled(size >= kg_domain::prefs::TERMINAL_FONT_MAX)
                .on_click(super::super::controls::dispatch(super::super::Bigger)),
        ));
    row(
        "terminal-font-size",
        "Terminal text size",
        "",
        buttons.into_any_element(),
    )
}

fn theme_group(window: &mut Window, cx: &mut App) -> Group {
    group(
        "theme",
        "Theme",
        vec![Row {
            wide: true,
            alias: Some("theme"),
            ..row(
                "theme",
                "Theme",
                "The colors of the window and the terminal.",
                theme_cards(window, cx),
            )
        }],
    )
}

#[derive(Clone, Copy)]
struct CardLook {
    accent: Hsla,
    line: Hsla,
    radius: Pixels,
    ring: Hsla,
    clear: Hsla,
    danger: Hsla,
    muted: Hsla,
}

fn theme_cards(window: &mut Window, cx: &mut App) -> AnyElement {
    let state = ThemeState::global(cx).read(cx);
    let (error, in_use, mode) = (
        state.error.clone(),
        state.settings.theme.clone(),
        state.mode,
    );
    let catalog = state.catalog.clone();
    let theme = cx.theme();
    let look = CardLook {
        accent: crate::theme::brand_color(cx),
        line: theme.border,
        radius: theme.radius,
        ring: theme.ring,
        clear: theme.transparent,
        danger: theme.danger,
        muted: theme.muted_foreground,
    };
    let cards = h_flex()
        .gap_3()
        .flex_wrap()
        .justify_end()
        .children(catalog.into_iter().map(|entry| match &entry.file {
            Ok(file) => {
                let selected = entry.id == in_use;
                theme_card(&entry, file, selected, mode, look, window, cx)
            }
            Err(problem) => broken_card(&entry.id, problem, look, cx),
        }));
    let mut column = v_flex().gap_2().child(theme_actions());
    if let Some(error) = error {
        column = column.child(
            div()
                .id("theme-error")
                .ui_container()
                .aria_label(SharedString::from(error.clone()))
                .type_small(cx)
                .text_color(look.danger)
                .child(error),
        );
    }
    column.child(cards).into_any_element()
}

fn theme_actions() -> impl IntoElement {
    h_flex()
        .gap_1()
        .justify_end()
        .child(control(
            "theme-new",
            Facts::button("New theme…"),
            Button::new("button-theme-new")
                .small()
                .outline()
                .label("New theme…")
                .on_click(super::super::controls::dispatch(super::super::NewTheme)),
        ))
        .child(control(
            "theme-folder",
            Facts::button("Open themes folder"),
            Button::new("button-theme-folder")
                .small()
                .outline()
                .label("Open themes folder")
                .on_click(super::super::controls::dispatch(super::super::OpenThemes)),
        ))
}

fn theme_card(
    entry: &CatalogEntry,
    file: &ThemeFile,
    selected: bool,
    mode: ThemeMode,
    look: CardLook,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let id = entry.id.clone();
    let source = match &entry.source {
        Source::Builtin => "Built in".to_owned(),
        Source::User => "Your file".to_owned(),
        Source::Plugin(name) => name.clone(),
    };
    let (clicked, activated) = (id.clone(), id.clone());
    let label = if selected {
        format!("{} theme, in use", file.name)
    } else {
        format!("{} theme", file.name)
    };
    let focus = super::super::tab_stop(window, cx, SharedString::from(format!("theme-focus-{id}")));
    let card = v_flex()
        .id(ElementId::Name(format!("theme-card-{id}").into()))
        .w(rems(7.25))
        .p_2()
        .gap_2()
        .rounded(look.radius)
        .border_2()
        .border_color(if selected { look.accent } else { look.line })
        .cursor_pointer()
        .child(
            h_flex()
                .justify_between()
                .items_center()
                .child(
                    h_flex()
                        .gap_1()
                        .children(swatches(file, mode).map(|color| swatch(color, look.line))),
                )
                .children(selected.then(|| Icon::new(IconName::Check).text_color(look.accent))),
        )
        .child(div().type_small(cx).child(file.name.clone()))
        .child(div().type_caption(cx).text_color(look.muted).child(source))
        .on_click(move |_, _, cx| save_theme(clicked.clone(), cx));
    let focused = focus.is_focused(window);
    div()
        .id(ElementId::Name(format!("theme-{id}").into()))
        .ui_observe_as(Facts {
            selected: Some(selected),
            focus: Some(focus.clone()),
            ..Facts::button(label.clone())
        })
        .key_context(ITEM)
        .on_action(move |_: &ActivateItem, _, cx| save_theme(activated.clone(), cx))
        .role(Role::Button)
        .aria_label(label)
        .rounded(look.radius)
        .border_2()
        .border_color(if focused { look.ring } else { look.clear })
        .child(card)
        .into_any_element()
}

fn broken_card(id: &str, problem: &str, look: CardLook, cx: &App) -> AnyElement {
    let label = format!("{id} theme, does not load: {problem}");
    div()
        .id(ElementId::Name(format!("theme-{id}").into()))
        .ui_observe_as(Facts {
            role: Role::Label,
            ..Facts::button(label.clone())
        })
        .aria_label(label)
        .border_2()
        .border_color(look.clear)
        .child(
            v_flex()
                .w(rems(7.25))
                .p_2()
                .gap_1()
                .rounded(look.radius)
                .border_2()
                .border_color(look.line)
                .child(div().type_small(cx).child(id.to_owned()))
                .child(
                    div()
                        .type_caption(cx)
                        .text_color(look.danger)
                        .child(problem.to_owned()),
                ),
        )
        .into_any_element()
}

fn save_theme(theme: String, cx: &mut App) {
    let mode = current_mode(cx);
    save(Op::Theme(ThemeOp::Set { theme, mode }), cx);
}

fn swatch(color: Hsla, line: Hsla) -> impl IntoElement {
    div()
        .size(rems(0.875))
        .rounded(rems(0.1875))
        .border_1()
        .border_color(line)
        .bg(color)
}

fn mode_group(cx: &mut App) -> Group {
    let in_use = current_mode(cx);
    let choices = [
        (Appearance::System, "system", "System"),
        (Appearance::Light, "light", "Light"),
        (Appearance::Dark, "dark", "Dark"),
    ];
    let control = h_flex().gap_1().children(choices.map(|(mode, id, label)| {
        let button = Button::new(ElementId::Name(format!("button-mode-{id}").into()))
            .small()
            .label(label)
            .on_click(move |_, _, cx| {
                let theme = ThemeState::global(cx).read(cx).settings.theme.clone();
                save(Op::Theme(ThemeOp::Set { theme, mode }), cx);
            });
        let button = if mode == in_use {
            button.primary()
        } else {
            button.outline()
        };
        control(
            format!("mode-{id}"),
            Facts {
                role: Role::RadioButton,
                selected: Some(mode == in_use),
                ..Facts::button(label)
            },
            button,
        )
    }));
    group(
        "mode",
        "Mode",
        vec![Row {
            alias: Some("mode"),
            ..row(
                "mode",
                "Mode",
                "Follows the system appearance.",
                control.into_any_element(),
            )
        }],
    )
}

fn current_mode(cx: &App) -> Appearance {
    ThemeState::global(cx).read(cx).settings.mode
}
