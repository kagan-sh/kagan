use gpui_kit::component::Root;
use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{TestAppContext, VisualTestContext, point, px, size};
use kg_core::Workspace;
use kg_domain::{Op, OpResult, Plugin, PluginId, PluginOp, ProjectId, ThreadId, ThreadOp};
use serde_json::json;

const ID: &str = "dev.kagan.canvas";

fn run(cx: &mut TestAppContext, op: Op) -> OpResult {
    let task = cx.update(|cx| Workspace::dispatch(op, cx));
    cx.run_until_parked();
    cx.foreground_executor().block_test(task).unwrap()
}

fn read(cx: &mut TestAppContext) -> String {
    let result = run(
        cx,
        Op::Thread(ThreadOp::CallTool {
            id: ThreadId("thr_swap".into()),
            name: "canvas_read".into(),
            input: json!({"name": "canvas"}),
        }),
    );
    let OpResult::ToolReply { text, is_error } = result else {
        panic!("tool reply: {result:?}");
    };
    assert!(!is_error, "{text}");
    text
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    tempfile::TempDir,
    VisualTestContext,
    gpui_kit::Entity<gpui_shell::ScriptView>,
) {
    let data = tempfile::tempdir().unwrap();
    super::seed_thread(data.path());
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../plugins/canvas")
        .canonicalize()
        .unwrap();
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    store
        .insert_plugin(&Plugin {
            id: PluginId(ID.into()),
            source: root.display().to_string(),
            root,
            commit: None,
            enabled: true,
            approved: json!({
                "powers": {"clipboard": {"read": true, "write": true}, "areas": ["database", "threads"]},
                "downloads": {}, "folders": {}
            }),
            error: None,
        })
        .unwrap();
    drop(store);
    let opened =
        cx.update(|cx| Workspace::open(data.path().into(), std::env::current_exe().unwrap(), cx));
    cx.foreground_executor().block_test(opened).unwrap();
    let started = cx.update(|cx| {
        gpui_kit::init(cx);
        gpui_shell::init(cx);
        crate::theme::init_in(data.path().join("themes"), cx);
        let mut catalog = gpui_shell::components::registry().unwrap();
        super::super::register(&mut catalog).unwrap();
        kg_plugins::init(cx, catalog.freeze().unwrap())
    });
    cx.foreground_executor().block_test(started).unwrap();
    let mut panel = None;
    let handle = cx.open_window(size(px(480.), px(640.)), |window, cx| {
        let host = kg_plugins::PluginHost::global(cx);
        let view = host.update(cx, |host, cx| {
            host.mount_panel(&PluginId(ID.into()), "canvas", window, cx)
                .unwrap()
        });
        panel = Some(view.clone());
        Root::new(view, window, cx)
    });
    let panel = panel.unwrap();
    let visual = VisualTestContext::from_window(handle.into(), cx);
    cx.run_until_parked();
    (data, visual, panel)
}

#[gpui_kit::test]
fn draws_on_real_canvas_panel_and_undoes_agent_edits(cx: &mut TestAppContext) {
    let (data, mut visual, panel) = mount(cx);
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(panel.read(cx).build_error(), None);
        let tool = window.find("tool-rect.button");
        assert!(tool.visible());
        assert!(tool.bounds().bottom() < px(120.));
        window.click("tool-rect.button", cx);
        window.drag(point(px(260.), px(200.)), point(px(420.), px(300.)), cx);
    });
    cx.run_until_parked();
    assert!(read(cx).contains("b1 box 228,168 160x100"));
    visual.update(|window, cx| window.click("canvas-undo.button", cx));
    cx.run_until_parked();
    assert!(read(cx).contains("is empty"));
    visual.update(|window, cx| window.click("canvas-redo.button", cx));
    cx.run_until_parked();
    assert!(read(cx).contains("b1 box"));

    let changed = run(
        cx,
        Op::Thread(ThreadOp::CallTool {
            id: ThreadId("thr_swap".into()),
            name: "canvas_edit".into(),
            input: json!({"name": "canvas", "ops": [{"op": "move", "id": "b1", "dx": 20, "dy": 0}]}),
        }),
    );
    assert!(matches!(
        changed,
        OpResult::ToolReply {
            is_error: false,
            ..
        }
    ));
    assert!(read(cx).contains("b1 box 248,168"));
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(panel.read(cx).build_error(), None);
        window.click("canvas-undo.button", cx);
    });
    cx.run_until_parked();
    assert!(read(cx).contains("b1 box 228,168"));
    let cli = run(
        cx,
        Op::Plugin(PluginOp::Cli {
            name: "canvas".into(),
            command: "show".into(),
            argv: vec!["canvas".into()],
            cwd: data.path().into(),
            thread_id: None,
            project_id: None,
        }),
    );
    assert!(matches!(cli, OpResult::Cli { exit_code: 0, stdout, .. } if stdout.trim() == read(cx)));

    let rejected = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: ThreadId("thr_swap".into()),
                name: "canvas_edit".into(),
                input: json!({"name": "canvas", "ops": [{"op": "update", "id": "b1", "patch": {"color": "purple"}}]}),
            }),
            cx,
        )
    });
    assert!(cx.foreground_executor().block_test(rejected).is_err());
    assert!(read(cx).contains("b1 box 228,168"));

    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(panel.read(cx).build_error(), None);
        window.click("tool-select.button", cx);
        window.drag(point(px(280.), px(220.)), point(px(300.), px(240.)), cx);
    });
    cx.run_until_parked();
    assert!(read(cx).contains("b1 box 248,188"));
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("tool-pen.button", cx);
        window.drag(point(px(280.), px(400.)), point(px(420.), px(460.)), cx);
    });
    cx.run_until_parked();
    assert!(read(cx).contains("p2 freehand"));
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(panel.read(cx).build_error(), None);
        window.click("tool-erase.button", cx);
        window.drag(point(px(300.), px(250.)), point(px(320.), px(250.)), cx);
    });
    cx.run_until_parked();
    let erased = read(cx);
    assert!(!erased.contains("b1 box"));
    assert!(erased.contains("p2 freehand"));
}

#[gpui_kit::test]
fn canvas_follows_project_context_and_tool_clicks_do_not_draw(cx: &mut TestAppContext) {
    let (_data, mut visual, panel) = mount(cx);
    visual.update(|window, cx| {
        window.render_frame(cx);
        window.click("tool-rect.button", cx);
        window.click("tool-select.button", cx);
        let host = kg_plugins::HostUi::global(cx).unwrap();
        host.update(cx, |host, cx| {
            host.set_context(
                window.window_handle(),
                Some(ThreadId("thr_swap".into())),
                Some(ProjectId("prj_swap".into())),
                true,
                &[PluginId(ID.into())],
                cx,
            );
        });
    });
    cx.run_until_parked();
    assert!(read(cx).contains("is empty"));
    let result = run(
        cx,
        Op::Thread(ThreadOp::CallTool {
            id: ThreadId("thr_swap".into()),
            name: "canvas_read".into(),
            input: json!({"name": "project"}),
        }),
    );
    assert!(
        matches!(result, OpResult::ToolReply { text, is_error: false } if text.contains("is empty"))
    );
    visual.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(panel.read(cx).build_error(), None);
    });
}
