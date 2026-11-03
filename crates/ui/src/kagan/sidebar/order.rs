use gpui_kit::component::ActiveTheme as _;
use gpui_kit::{
    App, AppContext as _, Bounds, Context, Entity, IntoElement, ParentElement as _, Pixels, Point,
    Render, SharedString, Styled as _, Window, div, px,
};
use kg_domain::{Project, ProjectId, ProjectKind};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Place {
    Before,
    After,
}

#[derive(Clone, PartialEq, Eq)]
pub(super) enum Before {
    End,
    Id(ProjectId),
}

impl Before {
    pub(super) fn into_option(self) -> Option<ProjectId> {
        match self {
            Self::End => None,
            Self::Id(id) => Some(id),
        }
    }
}

#[derive(Clone)]
pub(super) struct DragProject {
    pub(super) id: ProjectId,
    name: SharedString,
}

impl DragProject {
    pub(super) fn new(id: ProjectId, name: impl Into<SharedString>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl Render for DragProject {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py(px(2.))
            .rounded(cx.theme().radius)
            .bg(cx.theme().sidebar_accent)
            .text_color(cx.theme().muted_foreground)
            .child(self.name.clone())
    }
}

pub(super) fn place_at(bounds: Bounds<Pixels>, at: Point<Pixels>) -> Place {
    if at.y < bounds.center().y {
        Place::Before
    } else {
        Place::After
    }
}

pub(super) fn before_of(projects: &[Project], target: &ProjectId, place: Place) -> Option<Before> {
    let standard: Vec<&Project> = projects
        .iter()
        .filter(|project| project.kind == ProjectKind::Standard)
        .collect();
    let at = standard.iter().position(|project| &project.id == target)?;
    Some(match place {
        Place::Before => Before::Id(standard[at].id.clone()),
        Place::After => standard
            .get(at + 1)
            .map_or(Before::End, |project| Before::Id(project.id.clone())),
    })
}

pub(super) fn preview(drag: &DragProject, cx: &mut App) -> Entity<DragProject> {
    cx.new(|_| drag.clone())
}

pub(super) fn noop_gap(
    from: &ProjectId,
    target: &ProjectId,
    place: Place,
    projects: &[Project],
) -> bool {
    let standard: Vec<&ProjectId> = projects
        .iter()
        .filter(|project| project.kind == ProjectKind::Standard)
        .map(|project| &project.id)
        .collect();
    let Some(from_at) = standard.iter().position(|id| *id == from) else {
        return true;
    };
    let Some(target_at) = standard.iter().position(|id| *id == target) else {
        return true;
    };
    match place {
        Place::Before => target_at == from_at || target_at == from_at + 1,
        Place::After => target_at + 1 == from_at || target_at == from_at,
    }
}
