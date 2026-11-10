use crate::theme::TypeScale as _;

use gpui_kit::component::ThemeMode;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex,
};
use gpui_kit::{
    AnyElement, App, ElementId, FocusHandle, Hsla, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, rems,
};
use kg_domain::{Appearance, Op, PreferenceOp, ThemeFile, ThemeOp};
use serde_json::json;

use super::row::{Group, Row, group, row, save};
use super::{ActivateItem, ITEM, Kagan};
use crate::kagan::controls::control;
use crate::theme::{CatalogEntry, Density, Source, ThemeState, swatches};
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

pub(super) fn theme_group(window: &mut Window, cx: &mut App) -> Group {
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
pub(super) struct CardLook {
    accent: Hsla,
    line: Hsla,
    radius: Pixels,
    ring: Hsla,
    clear: Hsla,
    danger: Hsla,
    muted: Hsla,
}

pub(super) fn theme_cards(window: &mut Window, cx: &mut App) -> AnyElement {
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
    let mut focused = None;
    let mut cards = Vec::new();
    for entry in catalog {
        match &entry.file {
            Ok(file) => {
                let focus = super::super::tab_stop(
                    window,
                    cx,
                    SharedString::from(format!("theme-focus-{}", entry.id)),
                );
                if focus.is_focused(window) {
                    focused = Some(entry.id.clone());
                }
                let selected = entry.id == in_use;
                let card = Card {
                    entry: &entry,
                    file,
                    selected,
                    focus: &focus,
                };
                cards.push(theme_card(card, mode, look, window, cx));
            }
            Err(problem) => cards.push(broken_card(&entry.id, problem, look, cx)),
        }
    }
    cx.defer(move |cx| {
        ThemeState::global(cx).update(cx, |state, cx| {
            state.follow_focus(focused.as_deref(), cx);
        });
    });
    let cards = h_flex().gap_3().flex_wrap().justify_end().children(cards);
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

pub(super) fn theme_actions() -> impl IntoElement {
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

/// One theme card's entry and state: its file, whether it is in use, and its keyboard focus.
#[derive(Clone, Copy)]
pub(super) struct Card<'a> {
    pub(super) entry: &'a CatalogEntry,
    pub(super) file: &'a ThemeFile,
    pub(super) selected: bool,
    pub(super) focus: &'a FocusHandle,
}

pub(super) fn theme_card(
    card: Card,
    mode: ThemeMode,
    look: CardLook,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let Card {
        entry,
        file,
        selected,
        focus,
    } = card;
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
    let hover_id = id.clone();
    div()
        .id(ElementId::Name(format!("theme-{id}").into()))
        .on_hover(move |hovered, _, cx| {
            ThemeState::global(cx).update(cx, |state, cx| {
                if *hovered {
                    state.preview(&hover_id, cx);
                } else {
                    state.end_preview(cx);
                }
            });
        })
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

pub(super) fn broken_card(id: &str, problem: &str, look: CardLook, cx: &App) -> AnyElement {
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

pub(super) fn save_theme(theme: String, cx: &mut App) {
    ThemeState::global(cx).update(cx, ThemeState::commit_preview);
    let mode = current_mode(cx);
    save(Op::Theme(ThemeOp::Set { theme, mode }), cx);
}

pub(super) fn swatch(color: Hsla, line: Hsla) -> impl IntoElement {
    div()
        .size(rems(0.875))
        .rounded(rems(0.1875))
        .border_1()
        .border_color(line)
        .bg(color)
}

pub(super) fn terminal_size(kagan: &Kagan) -> Row {
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

pub(super) fn density_row(cx: &App) -> Row {
    let in_use = ThemeState::global(cx).read(cx).density;
    let choices = [
        (Density::Compact, "Compact"),
        (Density::Default, "Default"),
        (Density::Roomy, "Roomy"),
    ];
    let control = h_flex().gap_1().children(choices.map(|(density, label)| {
        let id = density.id();
        let button = Button::new(ElementId::Name(format!("button-density-{id}").into()))
            .small()
            .label(label)
            .on_click(move |_, _, cx| {
                save(
                    Op::Preference(PreferenceOp::Set {
                        key: kg_domain::prefs::DENSITY.into(),
                        value: json!(id),
                    }),
                    cx,
                );
            });
        let button = if density == in_use {
            button.primary()
        } else {
            button.outline()
        };
        control(
            format!("density-{id}"),
            Facts {
                role: Role::RadioButton,
                selected: Some(density == in_use),
                ..Facts::button(label)
            },
            button,
        )
    }));
    row(
        "density",
        "Density",
        "Compact fits more on screen. Roomy gives rows and spacing more air. Type sizes stay the same.",
        control.into_any_element(),
    )
}

pub(super) fn mode_group(cx: &mut App) -> Group {
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

pub(super) fn current_mode(cx: &App) -> Appearance {
    ThemeState::global(cx).read(cx).settings.mode
}
