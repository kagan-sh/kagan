use gpui_kit::component::Root;
use gpui_kit::{TestAppContext, VisualTestContext, px, size};
use gpui_shell::{ComponentRegistry, ShellRuntime};

mod button;
mod canvas;

fn registry() -> gpui_shell::FrozenComponentRegistry {
    build(false)
}

/// The catalog the app mounts with: the component library's `Button` and friends, then kagan's own.
fn app_registry() -> gpui_shell::FrozenComponentRegistry {
    build(true)
}

fn build(library: bool) -> gpui_shell::FrozenComponentRegistry {
    let mut registry = if library {
        gpui_shell::components::registry().unwrap()
    } else {
        ComponentRegistry::new(
            gpui_shell::COMPONENT_REGISTRY_API_VERSION,
            gpui_shell::DEFAULT_COMPONENT_MODULE,
        )
        .unwrap()
    };
    super::register(&mut registry).unwrap();
    registry.freeze().unwrap()
}

#[test]
fn catalog_has_all_four_components_and_real_typings() {
    let registry = registry();
    assert_eq!(
        registry
            .descriptors()
            .map(gpui_shell::ComponentDescriptor::name)
            .collect::<Vec<_>>(),
        [
            "KgButton",
            "Sketch",
            "ThreadTitle",
            "ThreadStatus",
            "Markdown"
        ]
    );
    let declarations = gpui_shell::type_declarations(&registry);
    assert!(declarations.contains("scene: SketchScene"));
    assert!(declarations.contains("thread_id: string"));
}

pub(super) fn mount(
    cx: &mut TestAppContext,
    source: &str,
) -> (
    tempfile::TempDir,
    VisualTestContext,
    gpui_kit::Entity<gpui_shell::ScriptView>,
) {
    let (directory, visual, view, _runtime) = mount_live(cx, source);
    (directory, visual, view)
}

/// Like `mount`, and keeps the runtime alive: a script's click reaches the script only while the
/// runtime lives, and the app keeps its runtime for as long as a window shows the script.
pub(super) fn mount_live(
    cx: &mut TestAppContext,
    source: &str,
) -> (
    tempfile::TempDir,
    VisualTestContext,
    gpui_kit::Entity<gpui_shell::ScriptView>,
    std::rc::Rc<gpui_shell::ShellRuntime>,
) {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(directory.path().join("main.js"), source).unwrap();
    mount_dir(cx, directory)
}

fn mount_dir(
    cx: &mut TestAppContext,
    directory: tempfile::TempDir,
) -> (
    tempfile::TempDir,
    VisualTestContext,
    gpui_kit::Entity<gpui_shell::ScriptView>,
    std::rc::Rc<gpui_shell::ShellRuntime>,
) {
    let workspace = cx.update(|cx| {
        kg_core::Workspace::open(
            directory.path().into(),
            std::env::current_exe().unwrap(),
            cx,
        )
    });
    cx.foreground_executor().block_test(workspace).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_shell::init(cx);
        crate::theme::init_in(directory.path().join("themes"), cx);
    });
    let projected = cx.update(|cx| {
        use gpui_kit::component::ActiveTheme as _;
        (
            cx.theme().background,
            cx.theme().foreground,
            cx.theme().primary,
        )
    });
    let runtime = cx
        .update(|cx| ShellRuntime::new_with_components(cx, app_registry()))
        .unwrap();
    cx.update(|cx| {
        use gpui_kit::component::ActiveTheme as _;
        assert_eq!(
            projected,
            (
                cx.theme().background,
                cx.theme().foreground,
                cx.theme().primary
            )
        );
    });
    let application = runtime
        .load_application(directory.path(), "main.js")
        .unwrap();
    let mut mounted = None;
    let handle = cx.open_window(size(px(1920.), px(1280.)), |window, cx| {
        let view = runtime.mount_application(&application, window, cx).unwrap();
        mounted = Some(view.clone());
        Root::new(view, window, cx)
    });
    let mut visual = VisualTestContext::from_window(handle.into(), cx);
    visual.update(|window, cx| window.draw(cx).clear(cx));
    let view = mounted.unwrap();
    visual.update(|_, cx| assert_eq!(view.read(cx).build_error(), None));
    (directory, visual, view, runtime)
}

#[gpui_kit::test]
fn script_can_paint_every_shape_and_markdown(cx: &mut TestAppContext) {
    let (_dir, mut visual, view) = mount(
        cx,
        r#"
import { View, div } from "gpui-kit";
import { Sketch, SketchScene, Markdown } from "gpui-component";
export default class Shapes extends View {
  init() {
    this.scene = SketchScene();
    this.scene.rect("box",0,0,100,80,"ink",2,17,false);
    this.scene.line("arrow",[[100,80],[200,120]],"both","ink",2,3,false);
    this.scene.text("label",50,20,["Sketch"],18,"middle","ink",false);
    this.scene.ink("pen",[[10,100,0.2],[30,110,0.8],[50,100,0.3]],"ink",8,false);
    this.scene.rect("box",0,0,100,80,"accent",2,17,true);
    this.scene.view(10,10,1);
  }
  render() {
    return div().size_full().child(new Markdown("message", "**Native** text"))
      .child(new Sketch("sketch",this.scene));
  }
}
"#,
    );
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(view.read(cx).build_error(), None);
    });
}

#[gpui_kit::test]
fn sketch_keys_replace_remove_and_clear_through_the_script(cx: &mut TestAppContext) {
    let (_dir, mut visual, view) = mount(
        cx,
        r#"
import { View, div } from "gpui-kit";
import { Sketch, SketchScene } from "gpui-component";
export default class Keys extends View {
  init() {
    this.scene = SketchScene();
    this.scene.rect("box",0,0,100,80,"ink",2,17,false);
    this.scene.rect("box",0,0,100,80,"accent",2,17,true);
    this.scene.remove("box",false);
    this.scene.clear();
  }
  render() { return div().child(new Sketch("sketch",this.scene)); }
}
"#,
    );
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(view.read(cx).build_error(), None);
    });
}

#[gpui_kit::test]
#[should_panic(expected = "Sketch rectangle dimensions must not be negative")]
fn a_sketch_rectangle_with_a_negative_side_fails_the_script(cx: &mut TestAppContext) {
    mount(
        cx,
        r#"
import { View, div } from "gpui-kit";
import { Sketch, SketchScene } from "gpui-component";
export default class Bad extends View {
  init() {
    this.scene = SketchScene();
    this.scene.rect("box",0,0,-1,10,"ink",2,1,false);
  }
  render() { return div().child(new Sketch("sketch",this.scene)); }
}
"#,
    );
}

#[gpui_kit::test]
#[should_panic(expected = "Sketch pressure must be between 0 and 1")]
fn a_sketch_pen_pressure_outside_zero_to_one_fails_the_script(cx: &mut TestAppContext) {
    mount(
        cx,
        r#"
import { View, div } from "gpui-kit";
import { Sketch, SketchScene } from "gpui-component";
export default class Bad extends View {
  init() {
    this.scene = SketchScene();
    this.scene.ink("pen",[[10,100,1.5]],"ink",8,false);
  }
  render() { return div().child(new Sketch("sketch",this.scene)); }
}
"#,
    );
}

#[gpui_kit::test]
fn sketch_drag_2000(cx: &mut TestAppContext) {
    use gpui_kit::{InputEvent, Modifiers, MouseButton, MouseMoveEvent, point};
    use std::time::Instant;
    super::sketch::BUILDS.with(|count| count.set(0));
    let (_dir, mut visual, view) = mount(
        cx,
        r#"
import { View, div } from "gpui-kit";
import { Sketch, SketchScene } from "gpui-component";
export default class Drag extends View {
  init() {
    this.scene = SketchScene();
    for (let i = 0; i < 2000; i++) {
      this.scene.rect(`b${i}`,(i%50)*36,Math.floor(i/50)*28,30,22,"ink",2,i+1,false);
    }
  }
  render() {
    return div().id("drag").size_full()
      .on_mouse_move((event,cx) => {
        this.scene.rect("b0",event.local_position.x,0,30,22,"ink",2,1,false);
        cx.notify();
      })
      .child(new Sketch("sketch",this.scene));
  }
}
"#,
    );
    // Mount's draw builds the 2,000 paths before this loop. Each frame rebuilds
    // the one dragged rectangle. Frames 0..20 are warm-up and stay out of the
    // sample; the budget figures below are frames 20..140 only.
    let mut samples = Vec::new();
    for frame in 0..140_u16 {
        let start = Instant::now();
        visual.update(|window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: point(px(100. + f32::from(frame)), px(100.)),
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.draw(cx).clear(cx);
            assert_eq!(view.read(cx).build_error(), None);
        });
        if frame >= 20 {
            samples.push((frame, start.elapsed()));
        }
    }
    let mut times: Vec<_> = samples.iter().map(|(_, time)| *time).collect();
    times.sort();
    let (max_frame, _) = samples
        .iter()
        .max_by_key(|(_, time)| *time)
        .expect("recorded frames");
    super::sketch::BUILDS.with(|count| {
        eprintln!("Sketch path builds: {}", count.get());
        assert_eq!(count.get(), 2140);
    });
    eprintln!(
        "Sketch 2,000 hand rectangles: frames={} (warm-up frames 0..20 excluded), p50={:.3}ms p95={:.3}ms max={:.3}ms at frame {max_frame}; pointer dispatch + typed single-shape edit + cache + GPUI layout/prepaint/paint (headless CPU, GPU presentation excluded)",
        times.len(),
        times[60].as_secs_f64() * 1000.,
        times[114].as_secs_f64() * 1000.,
        times[119].as_secs_f64() * 1000.,
    );
}

#[gpui_kit::test]
fn swapping_thread_part_in_one_slot_paints_the_new_part(cx: &mut TestAppContext) {
    let directory = tempfile::tempdir().unwrap();
    seed_thread(directory.path());
    std::fs::write(
        directory.path().join("main.js"),
        r#"
import { View, div } from "gpui-kit";
import { ThreadTitle, ThreadStatus } from "gpui-component";
export default class Swap extends View {
  init() { this.status = false; }
  render() {
    const child = this.status
      ? new ThreadStatus("thr_swap")
      : new ThreadTitle("thr_swap");
    return div().size_full().on_click((_, cx) => { this.status = true; cx.notify(); }).child(child);
  }
}
"#,
    )
    .unwrap();
    let (_dir, mut visual, view, _) = mount_dir(cx, directory);
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
        assert_eq!(view.read(cx).build_error(), None);
        assert!(observed(window, "thread-title").is_some_and(|element| element.visible()));
        assert!(observed(window, "thread-status").is_none());
        click_at(window, cx);
        window.draw(cx).clear(cx);
        assert_eq!(view.read(cx).build_error(), None);
        assert!(observed(window, "thread-title").is_none());
        assert!(observed(window, "thread-status").is_some_and(|element| element.visible()));
    });
}

fn observed(window: &gpui_kit::Window, name: &str) -> Option<crate::ui_registry::ElementSnapshot> {
    crate::ui_registry::find(window, &gpui_kit::ElementId::Name(name.to_owned().into()))
}

fn click_at(window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    use gpui_kit::{InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, point};
    let position = point(px(800.), px(600.));
    window.dispatch_event(
        MouseDownEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
            first_mouse: false,
        }
        .to_platform_input(),
        cx,
    );
    window.dispatch_event(
        MouseUpEvent {
            button: MouseButton::Left,
            position,
            modifiers: Modifiers::default(),
            click_count: 1,
        }
        .to_platform_input(),
        cx,
    );
}

fn seed_thread(dir: &std::path::Path) {
    use kg_domain::{
        Project, ProjectId, ProjectKind, Thread, ThreadId, ThreadStatus, Timestamp, Visibility,
    };
    std::fs::write(dir.join("registry.json"), r#"{"agents":[]}"#).unwrap();
    let store = kg_store::Store::open(dir.join("kagan.sqlite3")).unwrap();
    let project = Project {
        id: ProjectId("prj_swap".into()),
        kind: ProjectKind::Standard,
        name: "Project".into(),
        path: dir.to_path_buf(),
        git_remote_url: None,
    };
    store.put_project(&project).unwrap();
    store
        .put_thread(&Thread {
            id: ThreadId("thr_swap".into()),
            project_id: project.id,
            environment_id: None,
            title: "Swap thread".into(),
            title_locked: false,
            provider: "demo".into(),
            model: String::new(),
            reasoning_level: String::new(),
            status: ThreadStatus::Idle,
            visibility: Visibility::Visible,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            section_id: None,
            pinned: false,
            last_read_at: Some(Timestamp(1)),
            latest_attention_at: Timestamp(2),
            mark: kg_domain::ThreadMark::None,
            archived_at: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(2),
        })
        .unwrap();
}
