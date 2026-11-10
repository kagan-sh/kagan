use gpui_kit::component::Root;
use gpui_kit::{TestAppContext, VisualTestContext, px, size};
use gpui_shell::{ComponentRegistry, ShellRuntime};

mod button;
mod field;

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
fn catalog_has_every_component_and_real_typings() {
    let registry = registry();
    assert_eq!(
        registry
            .descriptors()
            .map(gpui_shell::ComponentDescriptor::name)
            .collect::<Vec<_>>(),
        [
            "KgButton",
            "KgInput",
            "KgTextarea",
            "WebView",
            "ThreadTitle",
            "ThreadStatus",
            "Markdown"
        ]
    );
    let declarations = gpui_shell::type_declarations(&registry);
    assert!(declarations.contains("thread_id: string"));
    assert!(declarations.contains("url(url: string): WebViewElement"));
    assert!(declarations.contains("store(keep: boolean): WebViewElement"));
}

#[test]
fn a_web_view_only_opens_an_allowed_host() {
    let hosts = [String::from("example.com")];
    let open = |url| super::webview::allowed(url, &hosts);
    assert!(super::webview::allowed("http://127.0.0.1:1/a", &[]).is_ok());
    assert!(super::webview::allowed("https://Localhost/a", &[]).is_ok());
    assert!(open("https://example.com/a").is_ok());
    assert!(open("https://EXAMPLE.com/a").is_ok());
    assert_eq!(
        super::webview::allowed("https://example.com/", &[]).unwrap_err(),
        super::webview::REFUSED_HOST
    );
    assert_eq!(
        open("file:///etc/passwd").unwrap_err(),
        super::webview::REFUSED_SCHEME
    );
    assert_eq!(
        open("http://127.0.0.1.example.com/").unwrap_err(),
        super::webview::REFUSED_HOST
    );
    assert!(open("javascript:alert(1)").is_err());
    assert!(open("about:blank").is_err());
    assert!(open("http:///missing").is_err());
}

/// Mounts `source` and keeps the runtime alive: a script's click reaches the script only while
/// the runtime lives, and the app keeps its runtime for as long as a window shows the script.
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
            None,
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
fn script_can_paint_markdown(cx: &mut TestAppContext) {
    let (_dir, mut visual, view, _runtime) = mount_live(
        cx,
        r#"
import { View, div } from "gpui-kit";
import { Markdown } from "gpui-component";
export default class Text extends View {
  render() {
    return div().size_full().child(new Markdown("message", "**Native** text"));
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
        assert!(observed(window, "thread-title").is_some_and(|element| element.visible));
        assert!(observed(window, "thread-status").is_none());
        click_at(window, cx);
        window.draw(cx).clear(cx);
        assert_eq!(view.read(cx).build_error(), None);
        assert!(observed(window, "thread-title").is_none());
        assert!(observed(window, "thread-status").is_some_and(|element| element.visible));
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
        sort: 0,
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
            status: ThreadStatus::Idle,
            visibility: Visibility::Visible,
            parent_thread_id: None,
            quiet: false,
            notify: kg_domain::ThreadNotify::Default,
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
