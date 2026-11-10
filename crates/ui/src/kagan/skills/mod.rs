use std::path::PathBuf;

use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::{
    AppContext as _, Context, Entity, EventEmitter, FocusHandle, Focusable as _,
    InteractiveElement as _, IntoElement, KeyBinding, ParentElement as _, Render, Styled as _,
    Subscription, Task, Window, actions, div,
};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, ProjectId, Skill, SkillOp, SkillSource};

use super::Kagan;
use super::screen::Center;

mod detail;
mod list;

pub(in crate::kagan) const CONTEXT: &str = "SkillsPage";
pub(in crate::kagan) const ROW_CONTEXT: &str = "SkillRow";

actions!(skills, [Back, OpenRow]);

pub(in crate::kagan) fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("escape", Back, Some(CONTEXT)),
        KeyBinding::new("enter", OpenRow, Some(ROW_CONTEXT)),
    ]
}

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
    // The SKILL.md text the editor was opened on; Save refuses if the file changed since.
    expected: String,
    creating: Option<Entity<InputState>>,
    confirming: bool,
    list_task: Option<Task<()>>,
    detail_task: Option<Task<()>>,
    // The page's own focus, so keys still reach it when a row or the detail is swapped in.
    focus: FocusHandle,
    // Set when the page swaps between list and detail; the next frame moves focus to the page.
    refocus: bool,
    _watch: Subscription,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::kagan) enum Sort {
    Name,
    Source,
}

pub(in crate::kagan) struct Open {
    pub(super) page: Entity<Page>,
    _fork: Subscription,
}

pub(crate) struct ForkSkill {
    project: ProjectId,
    prompt: String,
}

impl EventEmitter<ForkSkill> for Page {}

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
            list_task: None,
            detail_task: None,
            focus: cx.focus_handle(),
            refocus: false,
            expected: String::new(),
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
        self.list_task = Some(cx.spawn(async move |this, cx| {
            let result = listed.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(OpResult::Skills { skills, errors }) => {
                        page.listing = Some(skills);
                        page.errors = errors;
                    }
                    Ok(_) => {
                        page.listing = Some(Vec::new());
                        page.notice = Some("The skills did not load.".into());
                    }
                    Err(error) => {
                        page.listing = Some(Vec::new());
                        page.notice = Some(error.to_string());
                    }
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
        self.detail_task = Some(cx.spawn(async move |this, cx| {
            let result = shown.await;
            _ = this.update(cx, |page, cx| {
                match result {
                    Ok(OpResult::Skill { skill, body, files }) => {
                        page.selected = Some(Shown { skill, body, files });
                        page.notice = None;
                        page.refocus = true;
                    }
                    Ok(_) => page.notice = Some("The skill did not load.".into()),
                    Err(error) => page.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
    }

    fn fork(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(shown)) = (self.project.clone(), self.selected.as_ref()) else {
            return;
        };
        cx.emit(ForkSkill {
            project,
            prompt: fork_prompt(&shown.skill),
        });
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        self.refocus = true;
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
        self.detail_task = Some(cx.spawn(async move |this, cx| {
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
        self.detail_task = Some(cx.spawn_in(window, async move |this, cx| {
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
                                .default_value(text.clone())
                        });
                        let focus = editor.read(cx).focus_handle(cx);
                        window.focus(&focus, cx);
                        page.expected = text;
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
                expected: Some(self.expected.clone()),
                project_id: self.project.clone(),
            }),
            cx,
        );
        let name = shown.skill.name.clone();
        self.detail_task = Some(cx.spawn(async move |this, cx| {
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
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("skill-name"));
        let focus = input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        self.creating = Some(input);
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
        self.detail_task = Some(cx.spawn(async move |this, cx| {
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

fn fork_prompt(skill: &Skill) -> String {
    let source = match &skill.path {
        Some(path) => format!("Read its instructions first: {}.", path.display()),
        None => format!(
            "Read its instructions first with `kg skill show {}`.",
            skill.name
        ),
    };
    format!(
        "Make a new skill with its own name, based on the skill `{}`. {source} Do not edit or copy the source skill. Give the new skill its own folder and SKILL.md in the user skills folder, with `kg skill new <name>` and then `kg skill update <name> <text>`.",
        skill.name
    )
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
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if std::mem::take(&mut self.refocus) {
            let focus = self.focus.clone();
            window.defer(cx, move |window, cx| window.focus(&focus, cx));
        }
        let content = if self.selected.is_some() {
            detail::render(self, cx)
        } else {
            list::render(self, window, cx)
        };
        div()
            .key_context(CONTEXT)
            .track_focus(&self.focus)
            .size_full()
            .min_h_0()
            .on_action(cx.listener(|this, _: &Back, _, cx| this.back(cx)))
            .child(content)
    }
}

impl Kagan {
    pub(super) fn shows_skills(&self) -> bool {
        matches!(self.screen.current(), Center::Skills(..))
    }

    fn fork_skill(&mut self, request: &ForkSkill, window: &mut Window, cx: &mut Context<Self>) {
        self.open_start(&request.project, window, cx);
        self.start.prompt = Some(request.prompt.clone());
        cx.notify();
    }

    pub(crate) fn show_skills(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.shows_skills() {
            cx.notify();
            return;
        }
        self.screen.remember_center(window, cx);
        self.cancel_plugin_start();
        self.dismiss_thread_menus(cx);
        self.close_rename();
        let project = self.shown_project(cx);
        let page = cx.new(|cx| Page::new(project, window, cx));
        let fork = cx.subscribe_in(&page, window, |this, _, request: &ForkSkill, window, cx| {
            this.fork_skill(request, window, cx);
        });
        self.open_center(Center::Skills(Open { page, _fork: fork }), window, cx);
        cx.notify();
    }
}
