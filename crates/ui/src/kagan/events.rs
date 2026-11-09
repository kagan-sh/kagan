use super::screen::{Activation, Side};
use super::sidebar::{Event, Shows};
use super::surface::ClosePage;
use super::{FocusSidebar, Kagan, OpenSettings};
use kg_domain::ProjectOp;

use gpui_kit::{App, Context, Window};
use kg_domain::{Op, ProjectId, SurfaceOp};

impl Kagan {
    pub(super) fn sidebar_event(
        &mut self,
        event: &Event,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            Event::Thread(id) => self.select(id.clone(), window, cx),
            Event::Project(id) => self.show_start(id.clone(), window, cx),
            // Enter on a project starts the picked program there, as the README says. The program list
            // may still be loading, so the start then waits for the picker to have a program.
            Event::Start(id) => {
                self.show_start(id.clone(), window, cx);
                if self.picker.read(cx).command(cx).is_ok() {
                    self.start(window, cx);
                } else {
                    self.start_on_load = Some(id.clone());
                }
            }
            Event::Archive(id) => {
                self.select(id.clone(), window, cx);
                self.archive_thread(id, window, cx);
            }
            Event::ThreadActions(id) => {
                // Selecting closes the narrow overlay, which holds this row, so keep it open.
                let overlay = self.screen.drawn(Side::Left);
                self.select(id.clone(), window, cx);
                if overlay {
                    self.open_overlay(Side::Left, Activation::Focus, window, cx);
                }
                if let Some(shown) = self.shown_open_thread(cx) {
                    self.actions_menu = None;
                    self.sidebar
                        .update(cx, |sidebar, cx| sidebar.open_row_menu(&shown, window, cx));
                }
            }
            Event::RemoveProject(id) => self.remove_project(id.clone(), cx),
            Event::Reorder { id, before } => {
                self.reorder_project(id.clone(), before.clone(), cx);
            }
            Event::OpenSurface(open) => {
                let op = SurfaceOp::Open {
                    plugin: open.plugin.clone(),
                    id: open.id.clone(),
                };
                self.run_surface(op, window, cx);
            }
            Event::ClosePage => self.close_page(&ClosePage, window, cx),
            Event::Threads => self.run_surface(SurfaceOp::Threads, window, cx),
            Event::ManagePlugins => self.manage_plugins(window, cx),
            Event::NewPlugin => self.add_plugin(window, cx),
            Event::Settings => self.toggle_settings(&OpenSettings, window, cx),
            Event::RowPick(id, pick) => self.thread_pick(id, *pick, window, cx),
            Event::RowMenuClosed => self.close_thread_actions(window, cx),
            Event::Archived => self.show_archived(window, cx),
        }
    }

    fn remove_project(&mut self, project: ProjectId, cx: &mut Context<Self>) {
        self.run_then(
            Op::Project(ProjectOp::Remove { id: project }),
            cx,
            |this, result, window, cx| match result {
                Ok(kg_domain::OpResult::Warnings(warnings)) => {
                    this.refresh(window, cx);
                    this.note = Some(format!("Project removed. {}", warnings.join("; ")).into());
                    cx.notify();
                }
                result => this.finish(result, "remove project", window, cx),
            },
        );
    }

    fn reorder_project(
        &mut self,
        id: ProjectId,
        before: Option<ProjectId>,
        cx: &mut Context<Self>,
    ) {
        self.run(
            Op::Project(ProjectOp::Reorder { id, before }),
            "move project",
            cx,
        );
    }

    pub(super) fn shows(&self, cx: &App) -> Shows {
        Shows {
            thread: self.shown_thread(cx),
            page: self
                .screen
                .page()
                .map(|slot| (slot.plugin.clone(), slot.contribution.clone())),
            tool: self.tool.as_ref().map(|slot| slot.plugin.clone()),
            archived: self.shows_archived(),
            in_settings: self.in_settings(),
            waiting: self
                .replaced
                .chosen(kg_domain::Slot::Threads)
                .map(|_| self.waiting_count(cx))
                .filter(|count| *count > 0),
        }
    }

    pub(crate) fn focus_sidebar(
        &mut self,
        _: &FocusSidebar,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.tool.is_some() || self.in_settings() {
            self.run_surface(SurfaceOp::Threads, window, cx);
            if self.tool.is_some() {
                return;
            }
            let kagan = cx.entity();
            window.defer(cx, move |window, cx| {
                kagan.update(cx, |this, cx| this.focus_sidebar(&FocusSidebar, window, cx));
            });
            return;
        }
        if !self.panes.left {
            self.open_overlay(Side::Left, Activation::SidebarRow, window, cx);
            return;
        }
        let Some(id) = self.sidebar_target(cx) else {
            let focus = self.sidebar.read(cx).focus().clone();
            window.focus(&focus, cx);
            return;
        };
        self.sidebar
            .update(cx, |sidebar, cx| sidebar.reveal_row(&id, window, cx));
    }

    pub(super) fn sidebar_target(&self, cx: &App) -> Option<String> {
        let sidebar = self.sidebar.read(cx);
        let candidate = if self.shows_archived() {
            Some("archived".to_owned())
        } else {
            self.shown_thread(cx)
                .map(|id| format!("{}{id}", super::sidebar::THREAD_ROW))
                .or_else(|| {
                    sidebar
                        .chosen()
                        .map(|project| format!("{}{project}", super::sidebar::PROJECT_ROW))
                })
        };
        sidebar.row_or_first(candidate)
    }
}
