use super::*;
use gpui_kit::AppContext as _;
use gpui_shell::{
    ArgumentDescriptor, ArgumentSchema, ComponentArgument, ComponentDataValue as Value,
    ComponentDescriptor, ComponentPayload, ComponentRegistry, ConstructorDescriptor, RegistryError,
    StateDescriptor, StateMethodDescriptor,
};

type Method = fn(&mut Scene, &[Value]) -> anyhow::Result<()>;

fn number(value: &Value) -> anyhow::Result<f32> {
    let Value::Number(value) = value else {
        fail!("Sketch expects a number");
    };
    ensure!(
        value.is_finite() && value.abs() <= 1_000_000.,
        "Sketch coordinates must be finite and within ±1,000,000"
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "validated world coordinates become GPUI f32 geometry"
    )]
    Ok(*value as f32)
}

fn string(value: &Value) -> anyhow::Result<&str> {
    let Value::String(value) = value else {
        fail!("Sketch expects a string");
    };
    Ok(value)
}

fn boolean(value: &Value) -> anyhow::Result<bool> {
    let Value::Boolean(value) = value else {
        fail!("Sketch expects a boolean");
    };
    Ok(*value)
}

fn color(value: &Value) -> anyhow::Result<Color> {
    match string(value)? {
        "ink" => Ok(Color::Ink),
        "accent" => Ok(Color::Accent),
        "muted" => Ok(Color::Muted),
        "working" => Ok(Color::Working),
        "needs" => Ok(Color::Needs),
        "failed" => Ok(Color::Failed),
        value => fail!("Unknown Sketch color {value}"),
    }
}

fn width(value: &Value) -> anyhow::Result<f32> {
    let value = number(value)?;
    ensure!(
        (0.1..=100.).contains(&value),
        "Sketch width must be between 0.1 and 100"
    );
    Ok(value)
}

fn seed(value: &Value) -> anyhow::Result<u64> {
    let Value::Number(value) = value else {
        fail!("Sketch expects an integer seed");
    };
    ensure!(
        value.is_finite() && (0. ..=f64::from(u32::MAX)).contains(value) && value.fract() == 0.,
        "Sketch seed must be an unsigned 32-bit integer"
    );
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "validated unsigned 32-bit integer"
    )]
    Ok(*value as u64)
}

fn points(value: &Value, pressure: bool) -> anyhow::Result<Vec<Sample>> {
    let Value::Array(points) = value else {
        fail!("Sketch expects an array of points");
    };
    ensure!(
        !points.is_empty() && points.len() <= 20_000,
        "Sketch paths need 1 to 20,000 points"
    );
    points
        .iter()
        .map(|point| {
            let Value::Array(point) = point else {
                fail!("Sketch expects coordinate arrays");
            };
            match point.as_slice() {
                [x, y] => Ok(Sample::Position([number(x)?, number(y)?])),
                [x, y, p] if pressure => {
                    let p = number(p)?;
                    ensure!(
                        (0. ..=1.).contains(&p),
                        "Sketch pressure must be between 0 and 1"
                    );
                    Ok(Sample::Pressure([number(x)?, number(y)?, p]))
                }
                _ => fail!("Sketch points need x and y, and ink may include pressure"),
            }
        })
        .collect()
}

fn rect(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [key, x, y, w, h, stroke, width_value, seed_value, overlay] = args else {
        fail!("SketchScene.rect expects key, x, y, w, h, color, width, seed, overlay");
    };
    let (x, y, w, h) = (number(x)?, number(y)?, number(w)?, number(h)?);
    ensure!(
        w >= 0. && h >= 0.,
        "Sketch rectangle dimensions must not be negative"
    );
    ensure!(
        x + w <= 1_000_000. && y + h <= 1_000_000.,
        "Sketch rectangle must stay within ±1,000,000"
    );
    scene.put(
        Shape {
            key: string(key)?.into(),
            color: color(stroke)?,
            width: width(width_value)?,
            seed: seed(seed_value)?,
            geometry: Geometry::Rect { x, y, w, h },
        },
        boolean(overlay)?,
    )
}

fn line(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [key, vertices, heads, c, width_value, seed_value, overlay] = args else {
        fail!("SketchScene.line expects key, points, heads, color, width, seed, overlay");
    };
    let points: Vec<_> = points(vertices, false)?
        .into_iter()
        .map(|p| match p {
            Sample::Position(p) => p,
            Sample::Pressure([x, y, _]) => [x, y],
        })
        .collect();
    ensure!(points.len() >= 2, "Sketch lines need at least two points");
    let heads = match string(heads)? {
        "none" => Heads::None,
        "start" => Heads::Start,
        "end" => Heads::End,
        "both" => Heads::Both,
        value => fail!("Unknown arrow heads {value}"),
    };
    scene.put(
        Shape {
            key: string(key)?.into(),
            color: color(c)?,
            width: width(width_value)?,
            seed: seed(seed_value)?,
            geometry: Geometry::Line { points, heads },
        },
        boolean(overlay)?,
    )
}

fn ink(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [key, vertices, c, width_value, overlay] = args else {
        fail!("SketchScene.ink expects key, points, color, width, overlay");
    };
    scene.put(
        Shape {
            key: string(key)?.into(),
            color: color(c)?,
            width: width(width_value)?,
            seed: 0,
            geometry: Geometry::Ink {
                points: points(vertices, true)?,
            },
        },
        boolean(overlay)?,
    )
}

fn text(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [key, x, y, lines, size, anchor, c, overlay] = args else {
        fail!("SketchScene.text expects key, x, y, lines, size, anchor, color, overlay");
    };
    let Value::Array(lines) = lines else {
        fail!("Sketch text needs an array of lines");
    };
    ensure!(
        lines.len() <= 20_000,
        "Sketch text accepts at most 20,000 lines"
    );
    let lines = lines
        .iter()
        .map(|line| {
            let line = string(line)?;
            ensure!(
                !line.contains(['\n', '\r']),
                "Sketch text lines must not contain line breaks"
            );
            Ok(line.to_owned())
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let size = number(size)?;
    ensure!(
        (1. ..=1000.).contains(&size),
        "Sketch text size must be between 1 and 1000"
    );
    let anchor = match string(anchor)? {
        "start" => Anchor::Start,
        "middle" => Anchor::Middle,
        "end" => Anchor::End,
        value => fail!("Unknown text anchor {value}"),
    };
    scene.put(
        Shape {
            key: string(key)?.into(),
            color: color(c)?,
            width: stroke_width(),
            seed: 0,
            geometry: Geometry::Text {
                x: number(x)?,
                y: number(y)?,
                lines,
                size,
                anchor,
            },
        },
        boolean(overlay)?,
    )
}

fn remove(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [key, overlay] = args else {
        fail!("SketchScene.remove expects key and overlay");
    };
    let key = string(key)?;
    let shapes = if boolean(overlay)? {
        &mut scene.overlay
    } else {
        &mut scene.shapes
    };
    let ix = shapes.iter().position(|s| s.key == key).ok_or_else(|| {
        super::super::fail::ComponentError::new(format!("Sketch key {key} does not exist"))
    })?;
    shapes.remove(ix);
    Ok(())
}

fn clear(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    ensure!(args.is_empty(), "SketchScene.clear expects no arguments");
    scene.shapes.clear();
    scene.overlay.clear();
    Ok(())
}

fn view(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [x, y, scale] = args else {
        fail!("SketchScene.view expects x, y and scale");
    };
    let view = View {
        x: number(x)?,
        y: number(y)?,
        scale: number(scale)?,
    };
    ensure!(
        (0.01..=100.).contains(&view.scale),
        "Sketch scale must be between 0.01 and 100"
    );
    scene.view = view;
    Ok(())
}

fn style(scene: &mut Scene, args: &[Value]) -> anyhow::Result<()> {
    let [value] = args else {
        fail!("SketchScene.style expects hand or clean");
    };
    scene.style = match string(value)? {
        "hand" => Style::Hand,
        "clean" => Style::Clean,
        value => fail!("Unknown Sketch style {value}"),
    };
    Ok(())
}

pub(crate) fn register(registry: &mut ComponentRegistry) -> Result<(), RegistryError> {
    let methods: [(&str, &str, Method); 8] = [
        (
            "rect",
            "(key: string, x: number, y: number, w: number, h: number, color: 'ink' | 'accent' | 'muted' | 'working' | 'needs' | 'failed', width: number, seed: number, overlay: boolean): void",
            rect,
        ),
        (
            "line",
            "(key: string, points: number[][], heads: 'none' | 'start' | 'end' | 'both', color: 'ink' | 'accent' | 'muted' | 'working' | 'needs' | 'failed', width: number, seed: number, overlay: boolean): void",
            line,
        ),
        (
            "ink",
            "(key: string, points: number[][], color: 'ink' | 'accent' | 'muted' | 'working' | 'needs' | 'failed', width: number, overlay: boolean): void",
            ink,
        ),
        (
            "text",
            "(key: string, x: number, y: number, lines: string[], size: number, anchor: 'start' | 'middle' | 'end', color: 'ink' | 'accent' | 'muted' | 'working' | 'needs' | 'failed', overlay: boolean): void",
            text,
        ),
        ("remove", "(key: string, overlay: boolean): void", remove),
        ("clear", "(): void", clear),
        ("view", "(x: number, y: number, scale: number): void", view),
        ("style", "(style: 'hand' | 'clean'): void", style),
    ];
    registry.register_state(StateDescriptor::new("SketchScene","SketchScene",vec![],|args,_,cx| {
        if !args.is_empty() { return Err("SketchScene expects no arguments".into()); }
        Ok(Box::new(cx.new(|_|Scene::default())))
    }).with_documentation("Retained Sketch geometry owned by a GPUI entity. Shape methods add or replace one key atomically and preserve paint order; remove and clear release shapes. Overlay keys live in a separate layer and paint clean strokes. Numeric inputs are finite world coordinates, within ±1,000,000, and a rectangle's far corner stays inside that range. Width is 0.1–100, text size 1–1000, scale 0.01–100, and seed an unsigned 32-bit integer.")
    .with_methods(methods.into_iter().map(|(name,signature,method)|StateMethodDescriptor::new::<Entity<Scene>>(name,signature,move |scene,args,_,cx| {
        scene.update(cx,|scene,cx| { method(scene,args)?; cx.notify(); Ok(Value::Null) })
    })).collect()))?;
    registry.register(ComponentDescriptor::new("Sketch",Arc::new(Sketch))
        .with_constructors(vec![ConstructorDescriptor::new("Sketch",vec![super::super::string("id"),ArgumentDescriptor::new("scene",ArgumentSchema::Entity("SketchScene"))],|args| {
            let [ComponentArgument::String(id),state @ ComponentArgument::Entity {kind:"SketchScene",..}] = args else { return Err("new Sketch expects a stable id and SketchScene".into()); };
            super::super::nonempty(id)?;
            Ok(ComponentPayload::new((id.clone(), state.clone())))
        })]).with_documentation("Native boxes, arrow lines, text and perfect-freehand pen strokes from a typed SketchScene. Geometry is cached per component and shape key. Children may contain a text editor; pointer and keyboard input belong to the caller."))?;
    Ok(())
}
