#[cfg(test)]
thread_local! { pub(super) static BUILDS: std::cell::Cell<usize> = const {std::cell::Cell::new(0)}; }

mod state;
pub(super) use state::register;

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use gpui_kit::component::ActiveTheme as _;
use gpui_kit::component::WindowExt as _;
use gpui_kit::{
    App, Bounds, Entity, Hsla, ParentElement as _, Path, PathBuilder, Pixels, Styled as _, Window,
    canvas, div, point, px,
};
use gpui_shell::{ComponentMaterializer, MaterializeRequest, anyhow};
use perfect_freehand::{InputPoint, StrokeOptions, get_stroke};
use roughr::core::{OpType, OptionsBuilder};
use roughr::generator::Generator;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Scene {
    view: View,
    style: Style,
    shapes: Vec<Shape>,
    overlay: Vec<Shape>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct View {
    x: f32,
    y: f32,
    scale: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Style {
    Hand,
    Clean,
}

#[derive(Clone, Debug, PartialEq)]
struct Shape {
    key: String,
    color: Color,
    seed: u64,
    width: f32,
    geometry: Geometry,
}

fn stroke_width() -> f32 {
    2.
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Color {
    #[default]
    Ink,
    Accent,
    Muted,
    Working,
    Needs,
    Failed,
}

impl Color {
    fn resolve(self, cx: &App) -> Hsla {
        let owner = crate::theme::ThemeState::global(cx);
        let p = &owner.read(cx).palette;
        crate::theme::token_color(match self {
            Self::Ink => p.ink,
            Self::Accent => p.accent,
            Self::Muted => p.muted,
            Self::Working => p.working,
            Self::Needs => p.needs,
            Self::Failed => p.failed,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Geometry {
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
    },
    Line {
        points: Vec<[f32; 2]>,
        heads: Heads,
    },
    Text {
        x: f32,
        y: f32,
        lines: Vec<String>,
        size: f32,
        anchor: Anchor,
    },
    Ink {
        points: Vec<Sample>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Heads {
    None,
    Start,
    End,
    Both,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq)]
enum Sample {
    Position([f32; 2]),
    Pressure([f32; 3]),
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            view: View {
                x: 0.,
                y: 0.,
                scale: 1.,
            },
            style: Style::Hand,
            shapes: Vec::new(),
            overlay: Vec::new(),
        }
    }
}

impl Scene {
    fn put(&mut self, shape: Shape, overlay: bool) -> anyhow::Result<()> {
        ensure!(!shape.key.is_empty(), "Sketch keys must not be empty");
        let shapes = if overlay {
            &mut self.overlay
        } else {
            &mut self.shapes
        };
        if let Some(current) = shapes.iter_mut().find(|s| s.key == shape.key) {
            *current = shape;
        } else {
            ensure!(
                shapes.len() < 20_000,
                "Sketch accepts at most 20,000 shapes per layer"
            );
            shapes.push(shape);
        }
        Ok(())
    }
}

struct Cached {
    shape: Shape,
    style: Style,
    scale: f32,
    paths: Arc<Vec<Path<Pixels>>>,
}

#[derive(Default)]
struct Cache {
    shapes: HashMap<(bool, String), Cached>,
}

enum Drawing {
    Paths {
        paths: Arc<Vec<Path<Pixels>>>,
        color: Hsla,
    },
    Text(Shape),
}

impl Cache {
    fn prepare(&mut self, scene: &Scene, cx: &App) -> anyhow::Result<Vec<Drawing>> {
        let view = scene.view;
        let mut live = HashSet::new();
        let mut drawings = Vec::with_capacity(scene.shapes.len() + scene.overlay.len());
        for (overlay, shapes) in [(false, &scene.shapes), (true, &scene.overlay)] {
            let style = if overlay { Style::Clean } else { scene.style };
            for shape in shapes {
                let key = (overlay, shape.key.clone());
                live.insert(key.clone());
                let cached = self.shapes.entry(key).or_insert_with(|| Cached {
                    shape: shape.clone(),
                    style,
                    scale: 0.,
                    paths: Arc::default(),
                });
                if cached.shape != *shape
                    || cached.style != style
                    || cached.scale.to_bits() != view.scale.to_bits()
                {
                    cached.paths = Arc::new(paths(shape, style, view.scale)?);
                    cached.shape = shape.clone();
                    cached.style = style;
                    cached.scale = view.scale;
                }
                drawings.push(if matches!(shape.geometry, Geometry::Text { .. }) {
                    Drawing::Text(shape.clone())
                } else {
                    Drawing::Paths {
                        paths: cached.paths.clone(),
                        color: shape.color.resolve(cx),
                    }
                });
            }
        }
        self.shapes.retain(|key, _| live.contains(key));
        Ok(drawings)
    }
}

fn paths(shape: &Shape, style: Style, scale: f32) -> anyhow::Result<Vec<Path<Pixels>>> {
    #[cfg(test)]
    BUILDS.with(|count| count.set(count.get() + 1));
    let to_point = |[x, y]: [f32; 2]| point(px(x * scale), px(y * scale));
    let mut path = PathBuilder::stroke(px(shape.width * scale));
    match &shape.geometry {
        Geometry::Rect { x, y, w, h } => {
            let corners = [[*x, *y], [x + w, *y], [x + w, y + h], [*x, y + h]];
            if style == Style::Hand {
                rough(&mut path, &corners, true, shape.seed, scale)?;
            } else {
                path.add_polygon(&corners.map(to_point), true);
            }
        }
        Geometry::Line { points, heads } => {
            if style == Style::Hand {
                rough(&mut path, points, false, shape.seed, scale)?;
            } else {
                path.add_polygon(
                    &points.iter().copied().map(to_point).collect::<Vec<_>>(),
                    false,
                );
            }
            if matches!(heads, Heads::Start | Heads::Both) {
                arrow(&mut path, points[1], points[0], scale);
            }
            if matches!(heads, Heads::End | Heads::Both) {
                let end = points.len() - 1;
                arrow(&mut path, points[end - 1], points[end], scale);
            }
        }
        Geometry::Ink { points } => {
            let inputs: Vec<_> = points
                .iter()
                .map(|sample| match sample {
                    Sample::Position([x, y]) => {
                        InputPoint::Array([f64::from(*x), f64::from(*y)], None)
                    }
                    Sample::Pressure([x, y, p]) => {
                        InputPoint::Array([f64::from(*x), f64::from(*y)], Some(f64::from(*p)))
                    }
                })
                .collect();
            let outline = get_stroke(
                &inputs,
                &StrokeOptions {
                    size: Some(f64::from(shape.width)),
                    simulate_pressure: Some(
                        points.iter().all(|p| matches!(p, Sample::Position(_))),
                    ),
                    last: Some(true),
                    ..StrokeOptions::default()
                },
            );
            path = PathBuilder::fill();
            #[expect(
                clippy::cast_possible_truncation,
                reason = "bounded world coordinates become GPUI f32 geometry"
            )]
            let outline: Vec<_> = outline
                .into_iter()
                .map(|[x, y]| to_point([x as f32, y as f32]))
                .collect();
            path.add_polygon(&outline, true);
        }
        Geometry::Text { .. } => return Ok(Vec::new()),
    }
    Ok(vec![path.build()?])
}

fn rough(
    path: &mut PathBuilder,
    points: &[[f32; 2]],
    closed: bool,
    seed: u64,
    scale: f32,
) -> anyhow::Result<()> {
    let options = OptionsBuilder::default().seed(seed).build()?;
    let generator = Generator::default();
    // roughr's euclid point type is inferred from the generator's input.
    let points: Vec<_> = points.iter().map(|[x, y]| (*x, *y).into()).collect();
    let drawing = generator.linear_path(&points, closed, &Some(options));
    let p = |x, y| point(px(x * scale), px(y * scale));
    for set in drawing.sets {
        for op in set.ops {
            match (op.op, op.data.as_slice()) {
                (OpType::Move, [x, y]) => path.move_to(p(*x, *y)),
                (OpType::LineTo, [x, y]) => path.line_to(p(*x, *y)),
                (OpType::BCurveTo, [x1, y1, x2, y2, x, y]) => {
                    path.cubic_bezier_to(p(*x, *y), p(*x1, *y1), p(*x2, *y2));
                }
                _ => {
                    return Err(super::fail::ComponentError::new(
                        "roughr returned an invalid operation",
                    )
                    .into());
                }
            }
        }
    }
    Ok(())
}

fn arrow(path: &mut PathBuilder, from: [f32; 2], to: [f32; 2], scale: f32) {
    let angle = (to[1] - from[1]).atan2(to[0] - from[0]);
    for turn in [-0.5_f32, 0.5] {
        path.move_to(point(px(to[0] * scale), px(to[1] * scale)));
        path.line_to(point(
            px((to[0] - 10. * (angle + turn).cos()) * scale),
            px((to[1] - 10. * (angle + turn).sin()) * scale),
        ));
    }
}

struct SketchState {
    cache: Cache,
    _changes: gpui_kit::Subscription,
}

pub(super) struct Sketch;

impl ComponentMaterializer for Sketch {
    fn materialize(
        &self,
        mut request: MaterializeRequest<'_>,
    ) -> anyhow::Result<gpui_kit::AnyElement> {
        let (id, scene) = request
            .payload()
            .downcast_ref::<(String, gpui_shell::ComponentArgument)>()
            .ok_or_else(|| {
                super::fail::ComponentError::new("Sketch received incompatible scene data")
            })?
            .clone();
        let scene = request.with_state::<Entity<Scene>, _>(&scene, Clone::clone)?;
        let id = gpui_kit::SharedString::from(format!(
            "kagan-sketch:{:?}:{id}:{}",
            request.element_id(),
            scene.entity_id()
        ));
        let cache = request.with_window_app(|window, cx| {
            Ok(window
                .use_keyed_state(id, cx, |_, cx| SketchState {
                    cache: Cache::default(),
                    _changes: cx.observe(&scene, |_, _, cx| cx.notify()),
                })
                .clone())
        })?;
        let background = request.with_window_app(|_, cx| Ok(cx.theme().background))?;
        request.finish(
            div()
                .size_full()
                .overflow_hidden()
                .bg(background)
                .child(surface(scene, cache)),
        )
    }
}

fn surface(scene: Entity<Scene>, cache: Entity<SketchState>) -> impl gpui_kit::IntoElement {
    canvas(
        move |_, _, cx| {
            cache.update(cx, |state, cx| {
                let scene = scene.read(cx);
                (state.cache.prepare(scene, cx), scene.view)
            })
        },
        move |bounds, (drawings, view), window, cx| match drawings {
            Ok(drawings) => {
                let offset = bounds.origin + point(px(view.x), px(view.y));
                for drawing in drawings {
                    match drawing {
                        Drawing::Paths { paths, color } => {
                            for path in paths.iter() {
                                let mut path = path.clone();
                                path.bounds.origin += offset;
                                for vertex in &mut path.vertices {
                                    vertex.xy_position += offset;
                                }
                                window.paint_path(path, color);
                            }
                        }
                        Drawing::Text(shape) => paint_text(&shape, view, bounds, window, cx),
                    }
                }
            }
            Err(error) => window.push_notification(error.to_string(), cx),
        },
    )
    .size_full()
}

fn paint_text(
    shape: &Shape,
    view: View,
    bounds: Bounds<Pixels>,
    window: &mut Window,
    cx: &mut App,
) {
    use gpui_kit::{TextAlign, TextRun};
    let Geometry::Text {
        x,
        y,
        lines,
        size,
        anchor,
    } = &shape.geometry
    else {
        return;
    };
    let font_size = px(size * view.scale);
    for (line_ix, text) in lines.iter().enumerate() {
        let runs = [TextRun {
            len: text.len(),
            font: gpui_kit::font(cx.theme().font_family.clone()),
            color: shape.color.resolve(cx),
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
        let line = window
            .text_system()
            .shape_line(text.clone().into(), font_size, &runs, None);
        let shift = match anchor {
            Anchor::Start => px(0.),
            Anchor::Middle => line.width() / 2.,
            Anchor::End => line.width(),
        };
        #[expect(
            clippy::cast_precision_loss,
            reason = "line count is bounded by scene input size"
        )]
        let origin = bounds.origin
            + point(
                px(view.x + x * view.scale) - shift,
                px(view.y + y * view.scale + line_ix as f32 * f32::from(font_size) * 1.4),
            );
        if let Err(error) = line.paint(origin, font_size * 1.4, TextAlign::Left, None, window, cx) {
            window.push_notification(error.to_string(), cx);
        }
    }
}
