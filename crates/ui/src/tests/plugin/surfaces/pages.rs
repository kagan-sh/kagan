use crate::kagan::screen_read;

use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, TestAppContext, px, size};
use kg_core::Workspace;
use kg_domain::{
    Appearance, Op, PluginId, PluginOp, SurfacePlacement, ThreadOp, ThreadStatus, UiOp,
};

use super::{
    body, close_surface, height_of, install_broken, install_pair, item, open_surface, run, saved,
    screen, set_mode, surfaces, wide, width_of,
};
use crate::tests::kagan::{
    click, focused_of, label, open_with_plugins_then, press_key, run_shell, selected_of, shows,
    thread_record, wait_until,
};

const ALPHA: &str = "dev.test.alpha";
const BETA: &str = "dev.test.beta";
const BROKEN: &str = "dev.test.broken";

#[gpui_kit::test]
fn list_open_and_close_reach_every_placement(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.run_until_parked();

    let listed = surfaces(cx);
    assert_eq!(
        item(&listed, ALPHA, "tool").placement,
        SurfacePlacement::Left
    );
    assert_eq!(
        item(&listed, ALPHA, "page").placement,
        SurfacePlacement::Center
    );
    assert_eq!(
        item(&listed, ALPHA, "side").placement,
        SurfacePlacement::Right
    );
    assert_eq!(item(&listed, BETA, "tool").id, "tool");
    assert!(!item(&listed, ALPHA, "tool").open);
    let tool = format!("surface-item-{ALPHA}-tool");
    let page = format!("surface-item-{ALPHA}-page");
    assert!(shows(cx, window, &tool));
    assert!(shows(cx, window, &page));
    assert!(shows(cx, window, "plugins-manage"));
    assert!(!shows(cx, window, "plugins"));
    assert!(!shows(cx, window, &format!("surface-item-{ALPHA}-side")));
    assert!(shows(cx, window, "project-prj_kagan"));
    assert_eq!(label(cx, window, &tool), "Alpha tool");

    click(cx, window, &tool);
    wait_until(cx, |cx| shows(cx, window, "alpha-tool"));
    assert_eq!(selected_of(cx, window, &tool), Some(true));
    let opened = surfaces(cx);
    let tool_open = item(&opened, ALPHA, "tool").visible || item(&opened, BETA, "tool").visible;
    assert!(tool_open, "{opened:?}");

    let which = if item(&opened, ALPHA, "tool").open {
        ALPHA
    } else {
        BETA
    };
    close_surface(cx, which, "tool").unwrap();
    assert!(!shows(cx, window, "alpha-tool"));
    assert!(!shows(cx, window, "beta-tool"));

    open_and_close_the_page(cx, window);
    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-side"));
    assert!(shows(cx, window, "right-tab-bar"));
    assert!(item(&surfaces(cx), ALPHA, "side").visible);
    close_surface(cx, ALPHA, "side").unwrap();
    assert!(!shows(cx, window, "alpha-side"));

    let _ = kagan;
}

pub(in crate::tests) fn open_and_close_the_page(cx: &mut TestAppContext, window: AnyWindowHandle) {
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    assert!(shows(cx, window, "surface-title"));
    assert_eq!(label(cx, window, "surface-close"), "Close");
    assert_eq!(focused_of(cx, window, "surface-close"), Some(true));
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
    assert!(!shows(cx, window, "thread-title"));

    press_key(cx, window, "escape");
    assert!(!shows(cx, window, "surface-page"));
    open_surface(cx, ALPHA, "page").unwrap();
    press_key(cx, window, "space");
    assert!(!shows(cx, window, "surface-page"));
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-close"));
    click(cx, window, "surface-close");
    assert!(!shows(cx, window, "surface-page"));
}

#[gpui_kit::test]
fn a_narrow_window_still_opens_a_page_after_a_right_panel(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    cx.simulate_window_resize(window, size(px(528.), px(800.)));
    cx.run_until_parked();
    let right = saved(cx, "main.right-sidebar");

    open_surface(cx, ALPHA, "side").unwrap();
    wait_until(cx, |cx| shows(cx, window, &body(ALPHA, "side")));
    assert!(shows(cx, window, "right-overlay-panel"));
    assert_eq!(saved(cx, "main.right-sidebar"), right);

    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert!(width_of(cx, window, "center") >= 480. - 0.5);
    assert!(item(&surfaces(cx), ALPHA, "page").visible);
}

#[gpui_kit::test]
fn settings_and_a_page_do_not_show_together(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert_eq!(
        cx.update(|cx| kagan.read(cx).shown_thread(cx)),
        Some(id.clone())
    );

    click(cx, window, "settings-gear");
    cx.run_until_parked();
    assert!(shows(cx, window, "settings-page"));
    assert!(!shows(cx, window, "surface-page"));
    assert!(cx.update(|cx| kagan.read(cx).shown_thread(cx)).is_none());

    open_surface(cx, ALPHA, "page").unwrap();
    assert!(shows(cx, window, "surface-page"));
    assert!(!shows(cx, window, "settings-page"));
    assert_eq!(cx.update(|cx| kagan.read(cx).shown_thread(cx)), Some(id));
}

#[gpui_kit::test]
fn a_page_keeps_the_running_terminal_and_a_thread_switch_leaves_it(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let first = run_shell(cx);
    click(cx, window, &format!("thread-{}", first.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(first.clone())
    });
    let view = cx.update(|cx| screen_read::terminal_view(kagan.read(cx)));
    run(
        cx,
        Op::Thread(ThreadOp::Input {
            id: first.clone(),
            text: "printf SURFACEMARK\\n".into(),
            terminal: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| screen(cx, &first).contains("SURFACEMARK"));

    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    assert!(!shows(cx, window, "thread-title"));
    run(
        cx,
        Op::Thread(ThreadOp::Input {
            id: first.clone(),
            text: "printf PAGEUP\\n".into(),
            terminal: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| screen(cx, &first).contains("PAGEUP"));
    assert_eq!(thread_record(cx, &first).status, ThreadStatus::Active);
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );

    close_surface(cx, ALPHA, "page").unwrap();
    assert!(shows(cx, window, "thread-title"));
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );
    assert!(screen(cx, &first).contains("PAGEUP"));

    open_surface(cx, ALPHA, "page").unwrap();
    let second = run_shell(cx);
    click(cx, window, &format!("thread-{}", second.0));
    wait_until(cx, |cx| {
        cx.update(|cx| kagan.read(cx).shown_thread(cx)) == Some(second.clone())
    });
    assert!(!shows(cx, window, "surface-page"));
    assert_eq!(thread_record(cx, &first).status, ThreadStatus::Active);
    assert!(screen(cx, &first).contains("SURFACEMARK"));
    assert!(!item(&surfaces(cx), ALPHA, "page").open);
}

#[gpui_kit::test]
fn open_panel_props_stay_on_the_plugin_that_sent_them(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    let command = run(
        cx,
        Op::Plugin(PluginOp::Command {
            id: PluginId(ALPHA.into()),
            command: "open".into(),
            thread_id: None,
            project_id: None,
        }),
    );
    command.unwrap();
    wait_until(cx, |cx| label(cx, window, "alpha-tool") == "from-host");
    assert!(item(&surfaces(cx), ALPHA, "tool").visible);

    open_surface(cx, BETA, "tool").unwrap();
    wait_until(cx, |cx| shows(cx, window, "beta-tool"));
    assert_eq!(label(cx, window, "beta-tool"), "beta-ready");
    assert!(!shows(cx, window, "alpha-tool"));
}

#[gpui_kit::test]
fn reload_disable_and_removal_drop_one_plugin_and_leave_the_other(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));

    let entry = data.path().join("alpha").join("main.js");
    let next = std::fs::read_to_string(&entry)
        .unwrap()
        .replace("page-ready", "page-next");
    std::fs::write(&entry, next).unwrap();
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: PluginId(ALPHA.into()),
            thread_id: None,
        }),
    )
    .unwrap();
    wait_until(cx, |cx| label(cx, window, "alpha-page") == "page-next");
    assert_eq!(item(&surfaces(cx), BETA, "page").id, "page");

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ALPHA.into()),
        }),
    )
    .unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "surface-page"));
    assert!(!item(&surfaces(cx), BETA, "page").open);
    open_surface(cx, BETA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "beta-page"));

    let remove = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Remove {
                id: PluginId(BETA.into()),
                thread_id: None,
            }),
            cx,
        )
    });
    wait_until(cx, |cx| {
        cx.update(|cx| Workspace::global(cx).read(cx).approvals().count()) == 1
    });
    let approval = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .expect("approval")
            .id
            .clone()
    });
    let answered = cx.update(|cx| Workspace::answer_standalone(approval, "approve".into(), cx));
    cx.foreground_executor().block_test(answered).unwrap();
    cx.foreground_executor().block_test(remove).unwrap();
    cx.run_until_parked();
    assert!(!shows(cx, window, "surface-page"));
    assert!(surfaces(cx).iter().all(|item| item.plugin.0 != BETA));
    let _ = kagan;
}

#[gpui_kit::test]
fn a_page_that_fails_to_draw_offers_recovery_and_close(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open_with_plugins_then(cx, install_broken);
    wide(cx, window);
    let id = run_shell(cx);
    click(cx, window, &format!("thread-{}", id.0));
    let view = cx.update(|cx| screen_read::terminal_view(kagan.read(cx)));
    open_surface(cx, BROKEN, "page").unwrap();
    let failed = format!("panel-{BROKEN}-page");
    wait_until(cx, |cx| shows(cx, window, &format!("{failed}-reload")));
    assert!(shows(cx, window, &format!("{failed}-logs")));
    assert!(shows(cx, window, &format!("{failed}-disable")));
    assert!(shows(cx, window, "surface-close"));
    click(cx, window, "surface-close");
    assert!(!shows(cx, window, "surface-page"));
    assert!(shows(cx, window, "thread-title"));
    assert_eq!(
        cx.update(|cx| screen_read::terminal_view(kagan.read(cx))),
        view
    );
}

#[gpui_kit::test]
fn light_and_dark_keep_the_same_page_box(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "surface-page"));
    set_mode(cx, Appearance::Dark);
    let dark = (
        width_of(cx, window, "surface-page"),
        height_of(cx, window, "surface-title"),
    );
    set_mode(cx, Appearance::Light);
    let light = (
        width_of(cx, window, "surface-page"),
        height_of(cx, window, "surface-title"),
    );
    assert!((dark.0 - light.0).abs() < 0.5, "width {dark:?} {light:?}");
    assert!((dark.1 - light.1).abs() < 0.5, "title {dark:?} {light:?}");
    assert!(dark.0 > 0. && dark.1 > 0.);
}

pub(in crate::tests) fn origin_x(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .unwrap_or_else(|| panic!("{name} is not drawn"))
            .bounds
            .origin
            .x
            .as_f32()
    })
    .unwrap()
}

pub(in crate::tests) fn origin_y(
    cx: &mut TestAppContext,
    window: AnyWindowHandle,
    name: &str,
) -> f32 {
    cx.update_window(window, |_, window, _| {
        crate::ui_registry::find(window, &ElementId::Name(name.to_owned().into()))
            .unwrap_or_else(|| panic!("{name} is not drawn"))
            .bounds
            .origin
            .y
            .as_f32()
    })
    .unwrap()
}

#[gpui_kit::test]
fn a_page_body_lines_up_with_its_title(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open_with_plugins_then(cx, install_pair);
    wide(cx, window);
    open_surface(cx, ALPHA, "page").unwrap();
    wait_until(cx, |cx| shows(cx, window, "alpha-page"));
    let page = origin_x(cx, window, "surface-page");
    let title = origin_x(cx, window, "surface-title");
    let marker = origin_x(cx, window, &body(ALPHA, "page"));
    let content = origin_x(cx, window, "alpha-page");
    assert!(
        (content - title).abs() < 1.5,
        "content {content} title {title} page {page}"
    );
    assert!((marker - page).abs() < 1., "body {marker} page {page}");
    assert!(height_of(cx, window, &body(ALPHA, "page")) > 0.);
    let before = origin_y(cx, window, "alpha-page");
    run(
        cx,
        Op::Ui(UiOp::Scroll {
            element_id: "alpha-page".into(),
            within: None,
            dy: -200.,
            window: 0,
        }),
    )
    .unwrap();
    assert!(
        (origin_y(cx, window, "alpha-page") - before).abs() < 0.5,
        "a full page scrolled"
    );
}
