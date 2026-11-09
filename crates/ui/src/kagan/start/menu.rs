use crate::theme::TypeScale as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::{Disableable as _, Icon, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, Context, ElementId, Focusable as _, InteractiveElement as _,
    IntoElement, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, actions, div, px,
};

use super::{BranchPick, Branches, ProjectIntent, StartFolder, base_label};
use crate::kagan::Kagan;
use crate::kagan::controls::{control, note};
use crate::theme::row_height;
use crate::ui_observe::UiObserveExt as _;
use crate::ui_registry::Facts;
use kg_domain::BranchList;

pub(crate) const BRANCH_CONTEXT: &str = "BranchMenu";

actions!(kagan, [CloseBranchMenu]);

impl Kagan {
    pub(super) fn branch_control(
        &self,
        branches: &BranchList,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = self.branch_label(branches);
        let open = self.start.panel == super::Panel::Open;
        let muted = cx.theme().muted_foreground;
        v_flex()
            .w_full()
            .gap_1()
            .items_stretch()
            .child(control(
                "branch-from",
                Facts {
                    selected: Some(open),
                    disabled: self.start.starting,
                    ..Facts::button(label.clone())
                },
                Button::new("branch-from-button")
                    .small()
                    .outline()
                    .w_full()
                    .disabled(self.start.starting)
                    .tooltip(label.clone())
                    .child(
                        h_flex()
                            .w_full()
                            .items_center()
                            .gap_2()
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .text_ellipsis()
                                    .type_body(cx)
                                    .child(label.clone()),
                            )
                            .child(Icon::new(IconName::ChevronDown).small().text_color(muted)),
                    )
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_branch_menu(window, cx)),
                    ),
            ))
            .when(open, |rows| rows.child(self.branch_menu(branches, cx)))
            .into_any_element()
    }

    fn branch_label(&self, branches: &BranchList) -> SharedString {
        let text = if self.chosen_folder() == Some(StartFolder::Project) {
            match &self.start.pick {
                BranchPick::Current => match &branches.current {
                    Some(name) => format!("Current ({name})"),
                    None if branches.head.is_some() => "Current (detached)".into(),
                    None => "Current".into(),
                },
                BranchPick::Named(name) => format!("Checkout: {name}"),
                BranchPick::NewFrom(name) => format!("New branch from: {name}"),
            }
        } else {
            match &self.start.pick {
                BranchPick::Current => format!("Branch from: {}", base_label(branches)),
                BranchPick::Named(name) | BranchPick::NewFrom(name) => {
                    format!("Branch from: {name}")
                }
            }
        };
        text.into()
    }

    fn toggle_branch_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start.starting {
            return;
        }
        self.start.panel = match self.start.panel {
            super::Panel::Open => super::Panel::Closed,
            super::Panel::Closed => super::Panel::Open,
        };
        if self.start.panel == super::Panel::Open {
            self.start.intent = match (&self.start.pick, self.chosen_folder()) {
                (BranchPick::NewFrom(_), Some(StartFolder::Project)) => ProjectIntent::New,
                (BranchPick::Named(_), Some(StartFolder::Project)) => ProjectIntent::Checkout,
                _ => ProjectIntent::Current,
            };
            self.ensure_branch_query(window, cx);
            self.clear_branch_query(window, cx);
            if self.branch_search_shown() {
                self.focus_branch_query(window, cx);
            } else if let Some(focus) = self.start.branch_focus.clone() {
                window.focus(&focus, cx);
            }
        } else {
            self.picker
                .update(cx, |picker, cx| picker.focus(window, cx));
        }
        cx.notify();
    }

    fn set_intent(&mut self, intent: ProjectIntent, window: &mut Window, cx: &mut Context<Self>) {
        if self.start.starting {
            return;
        }
        self.start.intent = intent;
        self.clear_branch_query(window, cx);
        match intent {
            ProjectIntent::Current => self.apply_pick(BranchPick::Current, cx),
            ProjectIntent::New => {
                if !matches!(self.start.pick, BranchPick::NewFrom(_))
                    && let Branches::Loaded(list) = &self.start.branches
                    && let Some(base) = list.current.clone()
                {
                    self.start.pick = BranchPick::NewFrom(base);
                }
                self.focus_branch_query(window, cx);
                cx.notify();
            }
            ProjectIntent::Checkout => {
                self.focus_branch_query(window, cx);
                cx.notify();
            }
        }
    }

    fn take_branch(&mut self, name: String, cx: &mut Context<Self>) {
        let pick = if self.chosen_folder() == Some(StartFolder::Project)
            && self.start.intent == ProjectIntent::New
        {
            BranchPick::NewFrom(name)
        } else {
            BranchPick::Named(name)
        };
        self.apply_pick(pick, cx);
    }

    fn confirm_branch_query(&mut self, cx: &mut Context<Self>) {
        let Some(query) = &self.start.branch_query else {
            return;
        };
        let text = query.read(cx).value().to_string();
        let Branches::Loaded(list) = &self.start.branches else {
            return;
        };
        let head = text.trim().is_empty()
            && self.chosen_folder() != Some(StartFolder::Project)
            && list.current.is_none();
        let name = filtered(list, &text).into_iter().next();
        if head {
            self.apply_pick(BranchPick::Current, cx);
        } else if let Some(name) = name {
            self.take_branch(name, cx);
        }
    }

    fn branch_search_shown(&self) -> bool {
        self.chosen_folder() != Some(StartFolder::Project)
            || self.start.intent != ProjectIntent::Current
    }

    fn ensure_branch_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.start.branch_query.is_some() {
            return;
        }
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches"));
        self.start.branch_watch = Some(cx.subscribe(&query, |this, _, event, cx| match event {
            InputEvent::Change => cx.notify(),
            InputEvent::PressEnter { .. } => this.confirm_branch_query(cx),
            InputEvent::Focus | InputEvent::Blur => {}
        }));
        self.start.branch_query = Some(query);
    }

    fn clear_branch_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(query) = &self.start.branch_query {
            query.update(cx, |state, cx| state.set_value("", window, cx));
        }
    }

    fn focus_branch_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(query) = &self.start.branch_query {
            window.focus(&query.focus_handle(cx), cx);
        }
    }

    fn branch_menu(&self, branches: &BranchList, cx: &mut Context<Self>) -> AnyElement {
        let muted = cx.theme().muted_foreground;
        let project = self.chosen_folder() == Some(StartFolder::Project);
        let block = project
            .then(|| {
                super::branches::branch_move_block(
                    branches.checkout_block.clone(),
                    self.start.busy.is_busy(),
                )
            })
            .flatten();
        let query = self
            .start
            .branch_query
            .as_ref()
            .map(|query| query.read(cx).value().to_string())
            .unwrap_or_default();
        let show_list = self.branch_search_shown() && block.is_none();
        let focus = self.start.branch_focus.clone();
        v_flex()
            .id("branch-panel")
            .when_some(focus, |panel, focus| panel.track_focus(&focus))
            .ui_container()
            .key_context(BRANCH_CONTEXT)
            .on_action(cx.listener(|this, _: &CloseBranchMenu, window, cx| {
                this.close_branch_menu(window, cx);
                cx.notify();
            }))
            .w_full()
            .max_h(px(row_height(cx) * 16.))
            .overflow_y_scroll()
            .border_1()
            .border_color(cx.theme().border)
            .rounded(cx.theme().radius)
            .bg(cx.theme().background)
            .p_1()
            .gap_1()
            .when(project, |rows| {
                rows.child(section("Start from:", cx))
                    .child(self.intent_row(
                        "branch-current",
                        &current_intent_label(branches),
                        ProjectIntent::Current,
                        false,
                        cx,
                    ))
                    .child(self.intent_row(
                        "branch-new",
                        "New branch",
                        ProjectIntent::New,
                        block.is_some(),
                        cx,
                    ))
                    .child(self.intent_row(
                        "branch-checkout",
                        "Checkout",
                        ProjectIntent::Checkout,
                        block.is_some(),
                        cx,
                    ))
            })
            .children(block.map(|message| note("branch-block", message, muted, cx)))
            .when(show_list, |rows| {
                let heading = if project && self.start.intent == ProjectIntent::Checkout {
                    "Checkout:"
                } else {
                    "Branch from:"
                };
                rows.child(section(heading, cx))
                    .children(self.branch_search(cx))
                    .child(self.branch_list(branches, &query, cx))
            })
            .into_any_element()
    }

    fn intent_row(
        &self,
        id: &'static str,
        label: &str,
        intent: ProjectIntent,
        blocked: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let selected = self.start.intent == intent;
        let starting = self.start.starting;
        branch_row(
            id,
            label,
            selected,
            blocked || starting,
            cx.listener(move |this, _, window, cx| {
                if !blocked {
                    this.set_intent(intent, window, cx);
                }
            }),
            cx,
        )
    }

    fn branch_list(
        &self,
        branches: &BranchList,
        query: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let names = filtered(branches, query);
        let none = names.is_empty();
        let picked = match &self.start.pick {
            BranchPick::Current => branches.current.clone(),
            BranchPick::Named(name) | BranchPick::NewFrom(name) => Some(name.clone()),
        };
        let head = self.chosen_folder() != Some(StartFolder::Project)
            && branches.current.is_none()
            && query.trim().is_empty();
        let head_selected = matches!(self.start.pick, BranchPick::Current);
        v_flex()
            .gap_0()
            .children(head.then(|| {
                let label = base_label(branches);
                self.branch_choice("branch-head", &label, head_selected, cx, |this, cx| {
                    this.apply_pick(BranchPick::Current, cx);
                })
            }))
            .children(names.into_iter().map(|name| {
                let selected = picked.as_deref() == Some(name.as_str());
                let id = format!("branch-pick-{name}");
                let chosen = name.clone();
                self.branch_choice(&id, &name, selected, cx, move |this, cx| {
                    this.take_branch(chosen.clone(), cx);
                })
            }))
            .when(none && !head, |rows| {
                let text: SharedString = if query.trim().is_empty() {
                    "No branches found.".into()
                } else {
                    "No branches match.".into()
                };
                rows.child(note("branch-empty", text, cx.theme().muted_foreground, cx))
            })
            .into_any_element()
    }

    fn branch_search(&self, cx: &App) -> Option<AnyElement> {
        let query = self.start.branch_query.as_ref()?;
        Some(
            control(
                "branch-search",
                Facts::input("Search branches", query, cx),
                Input::new(query)
                    .text_size(px(crate::theme::type_body(cx)))
                    .small(),
            )
            .into_any_element(),
        )
    }

    fn branch_choice(
        &self,
        id: &str,
        label: &str,
        selected: bool,
        cx: &mut Context<Self>,
        on_pick: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> AnyElement {
        branch_row(
            id,
            label,
            selected,
            self.start.starting,
            cx.listener(move |this, _, _, cx| on_pick(this, cx)),
            cx,
        )
    }
}

fn current_intent_label(branches: &BranchList) -> SharedString {
    match &branches.current {
        Some(name) => format!("Current: {name}").into(),
        None if branches.head.is_some() => "Current (detached)".into(),
        None => "Current".into(),
    }
}

fn section(label: &'static str, cx: &App) -> AnyElement {
    div()
        .px_2()
        .type_caption(cx)
        .text_color(cx.theme().muted_foreground)
        .child(label)
        .into_any_element()
}

fn filtered(branches: &BranchList, query: &str) -> Vec<String> {
    let needle = query.trim().to_lowercase();
    let mut names: Vec<String> = branches
        .names
        .iter()
        .filter(|name| needle.is_empty() || name.to_lowercase().contains(&needle))
        .cloned()
        .collect();
    if let Some(current) = &branches.current {
        names.sort_by_key(|name| name != current);
    }
    names
}

fn branch_row(
    id: &str,
    label: &str,
    selected: bool,
    disabled: bool,
    on_click: impl Fn(&gpui_kit::ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let button = format!("{id}-button");
    let muted = cx.theme().muted_foreground;
    div()
        .id(SharedString::from(id))
        .ui_observe_as(Facts {
            disabled,
            selected: Some(selected),
            ..Facts::button(SharedString::from(label))
        })
        .w_full()
        .child(
            Button::new(ElementId::Name(button.into()))
                .small()
                .ghost()
                .w_full()
                .disabled(disabled)
                .when(selected, |button| button.bg(cx.theme().secondary))
                .child(
                    h_flex()
                        .w_full()
                        .items_center()
                        .gap_2()
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_ellipsis()
                                .child(SharedString::from(label)),
                        )
                        .when(selected, |row| {
                            row.child(Icon::new(IconName::Check).small().text_color(muted))
                        }),
                )
                .on_click(move |event, window, cx| {
                    if !disabled {
                        on_click(event, window, cx);
                    }
                }),
        )
        .into_any_element()
}
