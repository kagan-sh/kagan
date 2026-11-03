use crate::theme::TypeScale as _;
use std::collections::BTreeMap;
use std::rc::Rc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Entity, InteractiveElement as _,
    IntoElement, ParentElement as _, Styled as _, Window, actions, div, px,
};
use kg_domain::{AskField, PendingInteraction, PendingInteractionId, ThreadId};
use serde_json::{Value, json};

use super::Kagan;
use crate::kagan::shown::control;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;

actions!(kagan, [DismissAskPopup]);

pub(super) const CONTEXT: &str = "AskPopup";

#[derive(Clone)]
pub(super) struct Draft {
    texts: BTreeMap<String, Entity<InputState>>,
    flags: BTreeMap<String, bool>,
    picks: BTreeMap<String, Option<String>>,
    error: Option<String>,
}

impl Kagan {
    pub(super) fn sync_ask_drafts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let live: BTreeMap<PendingInteractionId, Vec<AskField>> = self
            .questions
            .read(cx)
            .questions
            .iter()
            .filter_map(|asked| {
                asked
                    .fields
                    .as_ref()
                    .map(|fields| (asked.id.clone(), fields.clone()))
            })
            .collect();
        self.ask_drafts.retain(|id, _| live.contains_key(id));
        if self
            .ask_hidden
            .as_ref()
            .is_some_and(|id| !live.contains_key(id))
        {
            self.ask_hidden = None;
        }
        for (id, fields) in live {
            self.ask_drafts
                .entry(id)
                .or_insert_with(|| Draft::new(&fields, window, cx));
        }
        let open = self.popup_ask(cx).is_some();
        if open && self.ask_popup.is_none() {
            let focus = cx.focus_handle().tab_stop(true);
            self.ask_popup = Some(focus.clone());
            cx.defer_in(window, move |_, window, cx| {
                window.focus(&focus, cx);
            });
        } else if !open && self.ask_popup.take().is_some() {
            self.focus_work(window, cx);
        }
    }

    pub(super) fn dismiss_ask_popup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.popup_ask(cx).map(|asked| asked.id.clone()) {
            self.ask_hidden = Some(id);
            self.ask_popup = None;
            self.focus_work(window, cx);
            cx.notify();
        }
    }

    pub(super) fn popup_ask<'a>(&'a self, cx: &'a App) -> Option<&'a PendingInteraction> {
        let shown = self.shown_thread(cx)?;
        self.questions
            .read(cx)
            .questions
            .iter()
            .find(|asked| asked.thread_id == shown && asked.fields.is_some())
            .filter(|asked| self.ask_hidden.as_ref() != Some(&asked.id))
    }
}

impl Draft {
    fn new(fields: &[AskField], window: &mut Window, cx: &mut Context<Kagan>) -> Self {
        let mut texts = BTreeMap::new();
        let mut flags = BTreeMap::new();
        let mut picks = BTreeMap::new();
        for field in fields {
            match field {
                AskField::String {
                    id,
                    default,
                    placeholder,
                    ..
                } => {
                    let default = default.clone();
                    let placeholder = placeholder.clone();
                    texts.insert(
                        id.clone(),
                        cx.new(|cx| {
                            let mut state = InputState::new(window, cx);
                            if let Some(default) = default {
                                state = state.default_value(default);
                            }
                            if let Some(placeholder) = placeholder {
                                state = state.placeholder(placeholder);
                            }
                            state
                        }),
                    );
                }
                AskField::Number { id, default, .. } => {
                    let default = *default;
                    texts.insert(
                        id.clone(),
                        cx.new(|cx| {
                            let mut state = InputState::new(window, cx);
                            if let Some(default) = default {
                                state = state.default_value(default.to_string());
                            }
                            state
                        }),
                    );
                }
                AskField::Boolean { id, default, .. } => {
                    flags.insert(id.clone(), default.unwrap_or(false));
                }
                AskField::Select {
                    id,
                    choices,
                    default,
                    ..
                } => {
                    let pick = default
                        .clone()
                        .or_else(|| choices.first().map(|choice| choice.id.clone()));
                    picks.insert(id.clone(), pick);
                }
            }
        }
        Self {
            texts,
            flags,
            picks,
            error: None,
        }
    }

    fn values(&self, fields: &[AskField], cx: &App) -> Result<BTreeMap<String, Value>, String> {
        let mut values = BTreeMap::new();
        for field in fields {
            let value = match field {
                AskField::String { id, .. } => {
                    let text = self
                        .texts
                        .get(id)
                        .map(|input| input.read(cx).value().to_string())
                        .unwrap_or_default();
                    Value::String(text)
                }
                AskField::Number { id, .. } => {
                    let text = self
                        .texts
                        .get(id)
                        .map(|input| input.read(cx).value().to_string())
                        .unwrap_or_default();
                    let text = text.trim();
                    let number: i64 = text
                        .parse()
                        .map_err(|_| format!("{} needs a whole number", field.label()))?;
                    json!(number)
                }
                AskField::Boolean { id, .. } => {
                    Value::Bool(self.flags.get(id).copied().unwrap_or(false))
                }
                AskField::Select { id, .. } => self
                    .picks
                    .get(id)
                    .cloned()
                    .flatten()
                    .map_or(Value::Null, Value::String),
            };
            values.insert(field.id().to_owned(), value);
        }
        Ok(values)
    }
}

pub(super) fn fields(
    asked: &PendingInteraction,
    draft: &Draft,
    kagan: &Entity<Kagan>,
    _window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let fields = asked.fields.as_deref().unwrap_or(&[]);
    let error = draft.error.clone();
    v_flex()
        .gap_2()
        .children(fields.iter().map(|field| match field {
            AskField::String { id, label, .. } | AskField::Number { id, label, .. } => {
                text_field(asked, draft, id, label, cx)
            }
            AskField::Boolean { id, label, .. } => flag_field(asked, draft, kagan, id, label),
            AskField::Select {
                id, label, choices, ..
            } => select_field(asked, draft, kagan, id, label, choices, cx),
        }))
        .children(error.map(|message| {
            div()
                .id(ElementId::Name(format!("ask-error-{}", asked.id.0).into()))
                .ui_container()
                .type_small()
                .text_color(cx.theme().danger)
                .child(message)
        }))
}

fn text_field(
    asked: &PendingInteraction,
    draft: &Draft,
    id: &str,
    label: &str,
    cx: &App,
) -> AnyElement {
    let input = draft.texts.get(id).cloned();
    let scope = format!("ask-field-{}-{}", asked.id.0, id);
    v_flex()
        .gap_1()
        .child(
            div()
                .type_small()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_owned()),
        )
        .children(input.map(|input| {
            div()
                .id(ElementId::Name(scope.clone().into()))
                .ui_observe_as(Facts::input(label.to_owned(), &input, cx))
                .child(
                    Input::new(&input)
                        .text_size(px(crate::theme::TYPE_BODY))
                        .into_any_element(),
                )
                .into_any_element()
        }))
        .into_any_element()
}

fn flag_field(
    asked: &PendingInteraction,
    draft: &Draft,
    kagan: &Entity<Kagan>,
    id: &str,
    label: &str,
) -> AnyElement {
    let on = draft.flags.get(id).copied().unwrap_or(false);
    let field_id = id.to_owned();
    let ask_id = asked.id.clone();
    let kagan = kagan.clone();
    let scope = format!("ask-field-{}-{}", asked.id.0, id);
    h_flex()
        .gap_2()
        .items_center()
        .child(
            div()
                .id(ElementId::Name(scope.into()))
                .ui_observe_as(Facts {
                    selected: Some(on),
                    ..Facts::button(label.to_owned())
                })
                .child(
                    Switch::new(ElementId::Name(
                        format!("ask-switch-{}-{}", asked.id.0, id).into(),
                    ))
                    .checked(on)
                    .on_click(move |checked: &bool, _, cx: &mut App| {
                        let checked = *checked;
                        kagan.update(cx, |kagan, cx| {
                            if let Some(draft) = kagan.ask_drafts.get_mut(&ask_id) {
                                draft.flags.insert(field_id.clone(), checked);
                                draft.error = None;
                                cx.notify();
                            }
                        });
                    }),
                ),
        )
        .child(div().type_body().child(label.to_owned()))
        .into_any_element()
}

fn select_field(
    asked: &PendingInteraction,
    draft: &Draft,
    kagan: &Entity<Kagan>,
    id: &str,
    label: &str,
    choices: &[kg_domain::InteractionOption],
    cx: &App,
) -> AnyElement {
    let picked = draft.picks.get(id).cloned().flatten();
    let shown = choices
        .iter()
        .find(|choice| Some(&choice.id) == picked.as_ref())
        .map_or_else(|| "None".to_owned(), |choice| choice.label.clone());
    let options: Vec<(Option<String>, String)> = std::iter::once((None, "None".to_owned()))
        .chain(
            choices
                .iter()
                .map(|choice| (Some(choice.id.clone()), choice.label.clone())),
        )
        .collect();
    let ask_id = asked.id.clone();
    let field_id = id.to_owned();
    let kagan_menu = kagan.clone();
    let button = Button::new(ElementId::Name(
        format!("ask-select-btn-{}-{}", asked.id.0, id).into(),
    ))
    .small()
    .outline()
    .label(shown.clone())
    .dropdown_menu(move |menu, _, _| {
        options.iter().fold(menu, |menu, (pick, name)| {
            let selected = *pick == picked;
            let item = match pick {
                Some(choice) => format!("ask-choice-{}-{field_id}-{choice}", ask_id.0),
                None => format!("ask-none-{}-{field_id}", ask_id.0),
            };
            let kagan = kagan_menu.clone();
            let ask_id = ask_id.clone();
            let field_id = field_id.clone();
            let pick = pick.clone();
            let name = name.clone();
            menu.item(
                PopupMenuItem::element(move |_, _| {
                    div()
                        .id(ElementId::Name(item.clone().into()))
                        .ui_observe_as(Facts {
                            selected: Some(selected),
                            ..Facts::button(name.clone())
                        })
                        .child(name.clone())
                })
                .on_click(move |_, _, cx| {
                    kagan.update(cx, |kagan, cx| {
                        if let Some(draft) = kagan.ask_drafts.get_mut(&ask_id) {
                            draft.picks.insert(field_id.clone(), pick.clone());
                            draft.error = None;
                            cx.notify();
                        }
                    });
                }),
            )
        })
    });
    v_flex()
        .gap_1()
        .child(
            div()
                .type_small()
                .text_color(cx.theme().muted_foreground)
                .child(label.to_owned()),
        )
        .child(h_flex().child(control(
            format!("ask-select-{}-{}", asked.id.0, id),
            Facts {
                value: Some(shown.into()),
                ..Facts::button(label.to_owned())
            },
            button,
        )))
        .into_any_element()
}

pub(super) fn popup(
    kagan: &mut Kagan,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> Option<AnyElement> {
    kagan.sync_ask_drafts(window, cx);
    let asked = kagan.popup_ask(cx)?.clone();
    let draft = kagan.ask_drafts.get(&asked.id)?.clone();
    let entity = cx.entity();
    let weak = entity.downgrade();
    let card =
        super::question::form_card(&asked, &draft, &entity, &weak, window, cx).into_any_element();
    let focus = kagan.ask_popup.clone()?;
    let scrim = cx.theme().background.opacity(0.72);
    let dismiss = cx.listener(|this, _: &DismissAskPopup, window, cx| {
        this.dismiss_ask_popup(window, cx);
    });
    Some(
        div()
            .id("ask-popup")
            .ui_container()
            .key_context(CONTEXT)
            .track_focus(&focus)
            .tab_stop(true)
            .capture_action(dismiss)
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .bg(scrim)
            .occlude()
            .child(
                div()
                    .id(ElementId::Name(format!("ask-popup-{}", asked.id.0).into()))
                    .ui_container()
                    .w(px(420.))
                    .max_w(px(560.))
                    .max_h(px(520.))
                    .overflow_y_scrollbar()
                    .child(card),
            )
            .into_any_element(),
    )
}

pub(super) fn collect(
    asked: &PendingInteraction,
    draft: &Draft,
    cx: &App,
) -> Result<BTreeMap<String, Value>, String> {
    let fields = asked.fields.as_deref().unwrap_or(&[]);
    draft.values(fields, cx)
}

pub(super) fn answer_with_fields(
    thread: &ThreadId,
    asked: &PendingInteraction,
    option: String,
    draft: &Draft,
    kagan: &gpui_kit::WeakEntity<Kagan>,
    cx: &mut App,
) {
    if crate::synthetic::refuse_for_person() {
        return;
    }
    let fields = match collect(asked, draft, cx) {
        Ok(fields) => fields,
        Err(error) => {
            let ask_id = asked.id.clone();
            let kagan = kagan.clone();
            _ = kagan.update(cx, |kagan, cx| {
                if let Some(draft) = kagan.ask_drafts.get_mut(&ask_id) {
                    draft.error = Some(error);
                    cx.notify();
                }
            });
            return;
        }
    };
    let ask_id = asked.id.clone();
    let kagan_clear = kagan.clone();
    _ = kagan_clear.update(cx, |kagan, _| {
        if let Some(draft) = kagan.ask_drafts.get_mut(&ask_id) {
            draft.error = None;
        }
    });
    let answered = kg_core::Workspace::answer_fields(thread, asked.id.clone(), option, fields, cx);
    let kagan = kagan.clone();
    cx.spawn(async move |cx| {
        if let Err(error) = answered.await {
            _ = kagan.update(cx, |kagan, cx| {
                kagan.note = Some(format!("Could not answer: {error}").into());
                cx.notify();
            });
        }
    })
    .detach();
}

pub(super) fn answer_callback(
    thread: ThreadId,
    asked: PendingInteraction,
    option: String,
    kagan: gpui_kit::WeakEntity<Kagan>,
) -> Rc<dyn Fn(&mut App)> {
    Rc::new(move |cx| {
        let Some(entity) = kagan.upgrade() else {
            return;
        };
        let draft = entity.read(cx).ask_drafts.get(&asked.id).cloned();
        let Some(draft) = draft else {
            kg_core::Workspace::answer(&thread, asked.id.clone(), option.clone(), cx).detach();
            return;
        };
        answer_with_fields(&thread, &asked, option.clone(), &draft, &kagan, cx);
    })
}
