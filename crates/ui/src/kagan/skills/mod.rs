use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{AppContext as _, Context, Entity, IntoElement, Render, Subscription, Task, Window};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProjectId, Skill, SkillOp, SkillSource};

use super::Kagan;
use super::screen::Center;

mod detail;
mod list;

// The skill page's own state. Kagan keeps only the center slot that holds this entity.
pub(in crate::kagan) struct Page {
    project: Option<ProjectId>,
    listing: Option<Vec<Skill>>,
    errors: Vec<String>,
    notice: Option<String>,
    query: Entity<InputState>,
    filter: Option<SkillSource>,
    sort: Sort,
    selected: Option<Shown>,
    editor: Option<Entity<TextareaState>>,
    creating: Option<Entity<InputState>>,
    confirming: bool,
    task: Option<Task<()>>,
    _watch: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::kagan) enum Sort {
    Name,
    Source,
}

pub(super) struct Shown {
    skill: Skill,
    body: String,
    files: Vec<PathBuf>,
}

impl Page {
    pub(in crate::kagan) fn new(
        project: Option<ProjectId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search skills"));
        let watch = cx.subscribe(&query, |_, _, event, cx| {
            if matches!(event, InputEvent::Change) {
                cx.notify();
            }
        });
        let mut page = Self {
            project,
            listing: None,
            errors: Vec::new(),
            notice: None,
            query,
            filter: None,
            sort: Sort::Name,
            selected: None,
            editor: None,
            creating: None,
            confirming: false,
            task: None,
            _watch: watch,
        };
        page.reload(cx);
        page
    }

    fn reload(&mut self, cx: &mut Context<Self>) {
        let listed = Workspace::dispatch(
            Op::Skill(SkillOp::List {
                project_id: self.project.clone(),
            }),
            cx,
        );
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = listed.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(OpResult::Skills { skills, errors }) => {
                        page.listing = Some(skills);
                        page.errors = errors;
                    }
                    Ok(_) => {}
                    Err(error) => page.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn open(&mut self, name: String, cx: &mut Context<Self>) {
        let shown = Workspace::dispatch(
            Op::Skill(SkillOp::Show {
                name,
                project_id: self.project.clone(),
            }),
            cx,
        );
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = shown.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(OpResult::Skill { skill, body, files }) => {
                        page.selected = Some(Shown { skill, body, files });
                        page.notice = None;
                    }
                    Ok(_) => {}
                    Err(error) => page.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.selected = None;
        self.editor = None;
        self.confirming = false;
        self.notice = None;
        cx.notify();
    }

    fn set_filter(&mut self, filter: Option<SkillSource>, cx: &mut Context<Self>) {
        self.filter = filter;
        cx.notify();
    }

    fn set_sort(&mut self, sort: Sort, cx: &mut Context<Self>) {
        self.sort = sort;
        cx.notify();
    }

    // A skill's folder is removed by Remove; the list reloads and the page returns to it.
    fn delete(&mut self, cx: &mut Context<Self>) {
        let Some(shown) = self.selected.as_ref() else {
            return;
        };
        let removed = Workspace::dispatch(
            Op::Skill(SkillOp::Remove {
                name: shown.skill.name.clone(),
                project_id: self.project.clone(),
            }),
            cx,
        );
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = removed.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(_) => {
                        page.selected = None;
                        page.confirming = false;
                        page.notice = None;
                        page.reload(cx);
                    }
                    Err(error) => {
                        page.confirming = false;
                        page.notice = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        }));
    }

    fn start_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self
            .selected
            .as_ref()
            .and_then(|shown| shown.skill.path.clone())
        else {
            return;
        };
        self.task = Some(cx.spawn_in(window, async move |this, cx| {
            let text = cx
                .background_executor()
                .spawn(async move { std::fs::read_to_string(path) })
                .await;
            _ = this.update_in(cx, |page, window, cx| {
                match text {
                    Ok(text) => {
                        let editor = cx.new(|cx| {
                            TextareaState::new(window, cx)
                                .auto_grow(8, 24)
                                .default_value(text)
                        });
                        page.editor = Some(editor);
                    }
                    Err(error) => page.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let (Some(editor), Some(shown)) = (self.editor.clone(), self.selected.as_ref()) else {
            return;
        };
        let text = editor.read(cx).value().to_string();
        let updated = Workspace::dispatch(
            Op::Skill(SkillOp::Update {
                name: shown.skill.name.clone(),
                text,
                project_id: self.project.clone(),
            }),
            cx,
        );
        let name = shown.skill.name.clone();
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = updated.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(_) => {
                        page.editor = None;
                        page.notice = None;
                        page.open(name, cx);
                        page.reload(cx);
                    }
                    Err(error) => page.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn start_new(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.creating = Some(cx.new(|cx| InputState::new(window, cx).placeholder("skill-name")));
        cx.notify();
    }

    fn create(&mut self, cx: &mut Context<Self>) {
        let Some(input) = self.creating.clone() else {
            return;
        };
        let name = input.read(cx).value().trim().to_owned();
        let made = Workspace::dispatch(
            Op::Skill(SkillOp::New {
                name: name.clone(),
                project: false,
                project_id: self.project.clone(),
            }),
            cx,
        );
        self.task = Some(cx.spawn(async move |this, cx| {
            let result = made.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(_) => {
                        page.creating = None;
                        page.notice = None;
                        page.reload(cx);
                        page.open(name, cx);
                    }
                    Err(error) => page.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn visible(&self, cx: &gpui_kit::App) -> Vec<Skill> {
        let query = self.query.read(cx).value().trim().to_lowercase();
        let mut skills: Vec<Skill> = self
            .listing
            .iter()
            .flatten()
            .filter(|skill| self.filter.is_none_or(|source| skill.source == source))
            .filter(|skill| {
                query.is_empty()
                    || skill.name.to_lowercase().contains(&query)
                    || skill.description.to_lowercase().contains(&query)
            })
            .cloned()
            .collect();
        match self.sort {
            Sort::Name => skills.sort_by(|a, b| a.name.cmp(&b.name)),
            Sort::Source => skills.sort_by(|a, b| {
                source_rank(a.source)
                    .cmp(&source_rank(b.source))
                    .then_with(|| a.name.cmp(&b.name))
            }),
        }
        skills
    }
}

pub(in crate::kagan) fn source_rank(source: SkillSource) -> u8 {
    match source {
        SkillSource::Bundled => 0,
        SkillSource::Plugin => 1,
        SkillSource::User => 2,
        SkillSource::Project => 3,
    }
}

impl Render for Page {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.selected.is_some() {
            detail::render(self, cx)
        } else {
            list::render(self, cx)
        }
    }
}

impl Kagan {
    pub(super) fn shows_skills(&self) -> bool {
        matches!(self.screen.current(), Center::Skills(_))
    }

    pub(crate) fn show_skills(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen.remember_center(window, cx);
        self.cancel_plugin_start();
        self.dismiss_thread_menus(cx);
        self.close_rename();
        let project = self.shown_project(cx);
        let page = cx.new(|cx| Page::new(project, window, cx));
        self.open_center(Center::Skills(page), window, cx);
        cx.notify();
    }
}
