use super::ask_form;
use super::{
    AddProject, CONTEXT, DismissLeftOverlay, DismissRightOverlay, Kagan, NewTheme, OpenThemes,
    Shown, commands, settings, web_panel,
};
use super::{archived, layout, overlays, right, screen, surface, update};
use crate::theme::{SpaceStep, TypeScale as _, space};

use crate::ui_observe::UiObserveExt as _;
use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::h_flex;
use gpui_kit::component::resizable::{ResizablePanel, h_resizable, resizable_panel};
use gpui_kit::component::v_flex;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, Context, InteractiveElement as _, IntoElement, ParentElement as _, Render,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use kg_domain::{Op, ThemeOp};

impl Kagan {
    pub(super) fn shown_plugin_views(&self, cx: &App) -> Vec<gpui_kit::EntityId> {
        let mut ids = Vec::new();
        let full = self.right.fill == right::Fill::Window && self.panes.right;
        if self.right_drawn()
            && let Some(right::Tab::Plugin(tab)) = right::selected_tab(self, cx)
            && let Some(view) = self
                .right
                .slots
                .iter()
                .find(|slot| slot.id == tab)
                .and_then(|slot| slot.view.as_ref())
        {
            ids.push(view.entity_id());
        }
        let left = !full
            && (self.panes.left || (self.screen.drawn(screen::Side::Left) && !self.in_settings()));
        if left && let Some(view) = self.slots.tool.as_ref().and_then(|slot| slot.view.as_ref()) {
            ids.push(view.entity_id());
        }
        if left && let Some(view) = self.slots.replaced.mounted_view(kg_domain::Slot::Threads) {
            ids.push(view.entity_id());
        }
        if !full {
            match self.screen.current() {
                screen::Center::Page(page) => {
                    if let Some(view) = page.view.as_ref() {
                        ids.push(view.entity_id());
                    }
                }
                screen::Center::Review(_) => {
                    if let Some(view) = self.slots.replaced.mounted_view(kg_domain::Slot::Review) {
                        ids.push(view.entity_id());
                    }
                }
                screen::Center::Settings => {
                    for (_, view) in self.slots.settings_views.values() {
                        ids.push(view.entity_id());
                    }
                }
                screen::Center::Work(_) | screen::Center::Archived(_) => {}
            }
        }
        ids
    }
}

impl Render for Kagan {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let main = self.center_body(window, cx);
        self.fit_to_window(window, cx);
        self.sync_browser(cx);
        crate::components::sync_pages(&self.shown_plugin_views(cx), cx);
        web_panel::sync(self, cx);
        let columns = if self.right.fill == right::Fill::Window && self.panes.right {
            div()
                .id("right-full-column")
                .size_full()
                .child(right::render(self, window, cx))
                .into_any_element()
        } else {
            self.columns(main, window, cx)
        };
        v_flex()
            .id("kagan")
            .type_body(cx)
            .key_context(window_context(self, cx).as_str())
            .track_focus(&self.focus)
            .size_full()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .on_action(cx.listener(Self::toggle_details_action))
            .on_action(cx.listener(Self::toggle_fill_action))
            .on_action(cx.listener(Self::toggle_form_action))
            .on_action(cx.listener(Self::new_thread))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::new_terminal))
            .on_action(cx.listener(Self::preview_file))
            .on_action(cx.listener(Self::close_terminal))
            .on_action(cx.listener(Self::start_again))
            .on_action(cx.listener(Self::save_diagnostics))
            .on_action(cx.listener(Self::run_command))
            .on_action(cx.listener(Self::toggle_pinned))
            .on_action(cx.listener(Self::archive))
            .on_action(cx.listener(|_, _: &AddProject, window, cx| Self::pick_project(window, cx)))
            .on_action(cx.listener(Self::focus_sidebar))
            .on_action(cx.listener(Self::restore_thread))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_right))
            .on_action(cx.listener(Self::bigger))
            .on_action(cx.listener(Self::smaller))
            .on_action(cx.listener(Self::actual_size))
            .on_action(cx.listener(Self::use_built_in_sidebar))
            .on_action(cx.listener(Self::use_built_in_review))
            .on_action(cx.listener(|_, _: &NewTheme, window, cx| {
                let id = crate::theme::copy_id(cx);
                Self::open_answer(Op::Theme(ThemeOp::New { id, from: None }), window, cx);
            }))
            .on_action(cx.listener(|_, _: &OpenThemes, window, cx| {
                Self::open_answer(Op::Theme(ThemeOp::Folder), window, cx);
            }))
            .on_action(cx.listener(|this, _: &DismissRightOverlay, window, cx| {
                this.dismiss_overlay(screen::Side::Right, window, cx);
            }))
            .on_action(cx.listener(|this, _: &DismissLeftOverlay, window, cx| {
                this.dismiss_overlay(screen::Side::Left, window, cx);
            }))
            .on_action(cx.listener(Self::toggle_settings))
            .on_action(cx.listener(Self::use_project_folder))
            .on_action(cx.listener(Self::use_worktree_folder))
            .on_action(cx.listener(Self::retry_branches_action))
            .on_action(cx.listener(Self::retry_programs))
            .on_action(cx.listener(Self::toggle_custom_command))
            .on_action(cx.listener(Self::open_thread_actions))
            .on_action(cx.listener(Self::begin_rename))
            .on_action(cx.listener(Self::close_settings))
            .on_action(cx.listener(Self::close_page))
            .on_action(commands::open_list)
            .on_action(commands::search_threads)
            .on_action(|_: &settings::NextItem, window, cx| window.focus_next(cx))
            .on_action(|_: &settings::PreviousItem, window, cx| window.focus_prev(cx))
            .child(self.title_bar(window, cx))
            .child(window_body(self, columns, window, cx))
            .child(self.activity_strip(window, cx))
    }
}

fn window_body(
    kagan: &mut Kagan,
    columns: impl IntoElement,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> impl IntoElement {
    h_flex()
        .flex_1()
        .min_h_0()
        .relative()
        .child(kagan.rail_side(window, cx))
        .child(div().flex_1().min_w_0().h_full().child(columns))
        .children(kagan.left_layer(window, cx))
        .children(kagan.right_layer(window, cx))
        .children(commands::layer(kagan, window, cx))
}

impl Kagan {
    pub(super) fn columns(
        &mut self,
        main: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let layout = self.layout;
        h_resizable("columns")
            .with_state(&self.columns)
            .when(self.panes.left, |columns| {
                columns.child(
                    resizable_panel()
                        .flex_none()
                        .size(layout.left_width)
                        .size_range(px(layout::LEFT_RANGE.0)..px(layout::LEFT_RANGE.1))
                        .child(self.left_side(window, cx)),
                )
            })
            .child(self.center_column(main, window, cx))
            .when(self.panes.right, |columns| {
                columns.child(
                    resizable_panel()
                        .flex_none()
                        .size(layout.right_width)
                        .size_range(px(right::WIDTH_RANGE.0)..px(right::WIDTH_RANGE.1))
                        .child(right::render(self, window, cx)),
                )
            })
            .into_any_element()
    }

    pub(super) fn center_column(
        &mut self,
        main: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> ResizablePanel {
        let popup = ask_form::popup(self, window, cx);
        resizable_panel()
            .size_range(px(layout::CENTER_MIN)..px(f32::MAX))
            .child(
                v_flex()
                    .id("center")
                    .ui_container()
                    .relative()
                    .size_full()
                    .min_w_0()
                    .when_some(self.note.clone(), |main, note| {
                        main.child(
                            div()
                                .id("note")
                                .ui_container()
                                .aria_label(note.clone())
                                .px(px(space(SpaceStep::Lg, cx)))
                                .py(px(space(SpaceStep::Sm, cx)))
                                .type_body(cx)
                                .text_color(cx.theme().danger)
                                .child(note),
                        )
                    })
                    .children(update::notice(cx))
                    .children(
                        self.plugin_prompt_visible(cx)
                            .then(|| Self::plugin_prompt_notice(cx)),
                    )
                    .child(main)
                    .children(overlays::render(self, cx))
                    .children(popup)
                    .when_some(self.toast.clone(), |main, toast| {
                        main.child(Self::render_toast(&toast, cx))
                    }),
            )
    }

    pub(super) fn center_body(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.screen.current() {
            screen::Center::Settings => settings::main(self, window, cx).into_any_element(),
            screen::Center::Page(_) => surface::page(self, window, cx),
            screen::Center::Review(_) => {
                let replaced = self.replaced_body(kg_domain::Slot::Review, window, cx);
                self.render_review(replaced, window, cx)
            }
            screen::Center::Archived(_) => archived::list(self, window, cx).into_any_element(),
            screen::Center::Work(Shown::Thread(view)) => {
                self.render_thread(view, cx).into_any_element()
            }
            screen::Center::Work(Shown::Start(project)) => {
                self.render_start(project, cx).into_any_element()
            }
            screen::Center::Work(Shown::Nothing) => Self::render_welcome(cx).into_any_element(),
        }
    }
}
fn window_context(kagan: &Kagan, cx: &App) -> String {
    let mut context = if kagan.in_settings() {
        format!("{CONTEXT} {}", settings::CONTEXT)
    } else {
        CONTEXT.to_owned()
    };
    if kagan.shown_thread(cx).is_some() {
        context = format!("{context} {}=true", crate::plugins::THREAD);
    }
    if kagan.shown_project(cx).is_some() {
        context = format!("{context} {}=true", crate::plugins::PROJECT);
    }
    if kagan.screen.drawn(screen::Side::Right) {
        context.push(' ');
        context.push_str(layout::RIGHT_OVERLAY);
    }
    context
}
