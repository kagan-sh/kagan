use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{
    InteractionOption, Op, OpResult, PreferenceOp, ProjectId, SectionOp, ThreadId, ThreadOp,
};
use serde_json::json;

use super::{
    click, open, row_top, run_op, run_shell, selected_of, set_preference, shows, thread_record,
    wait_until,
};

fn later(cx: &mut TestAppContext) {
    cx.executor().advance_clock(Duration::from_millis(50));
}

fn above(
    cx: &mut TestAppContext,
    window: gpui_kit::AnyWindowHandle,
    higher: &ThreadId,
    lower: &ThreadId,
) -> bool {
    let higher = format!("thread-{}", higher.0);
    let lower = format!("thread-{}", lower.0);
    shows(cx, window, &higher)
        && shows(cx, window, &lower)
        && row_top(cx, window, &higher) < row_top(cx, window, &lower)
}

fn ask(cx: &mut TestAppContext, id: &ThreadId) {
    later(cx);
    run_op(
        cx,
        Op::Thread(ThreadOp::Ask {
            id: id.clone(),
            title: "Still there?".into(),
            options: vec![
                InteractionOption {
                    id: "yes".into(),
                    label: "Yes".into(),
                    role: None,
                },
                InteractionOption {
                    id: "no".into(),
                    label: "No".into(),
                    role: None,
                },
            ],
            fields: None,
        }),
    );
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_list_menu_hides_an_empty_project_and_an_empty_section(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let thread = run_shell(cx);
    assert!(shows(cx, window, "project-personal"));
    assert!(shows(cx, window, "project-prj_kagan"));

    run_op(
        cx,
        Op::Section(SectionOp::Create {
            name: "Later".into(),
            thread_id: None,
        }),
    );
    cx.run_until_parked();
    let section = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .sections()
            .iter()
            .find(|section| section.name == "Later")
            .expect("section")
            .id
            .clone()
    });
    let empty = format!("section-{section}");
    wait_until(cx, |cx| shows(cx, window, &empty));

    click(cx, window, "sidebar-list-options");
    assert_eq!(
        selected_of(cx, window, "sidebar-hide-empty-projects"),
        Some(false)
    );
    click(cx, window, "sidebar-hide-empty-projects");
    wait_until(cx, |cx| !shows(cx, window, "project-personal"));
    assert!(shows(cx, window, "project-prj_kagan"));

    click(cx, window, "sidebar-list-options");
    click(cx, window, "sidebar-hide-empty-sections");
    wait_until(cx, |cx| !shows(cx, window, &empty));

    run_op(
        cx,
        Op::Section(SectionOp::File {
            thread_id: thread,
            section_id: Some(section.clone()),
        }),
    );
    cx.run_until_parked();
    let filed = format!("section-{}-{section}", ProjectId("prj_kagan".into()));
    wait_until(cx, |cx| shows(cx, window, &filed));
    assert!(!shows(cx, window, &empty));
}

#[gpui_kit::test]
fn attention_and_unread_reorder_threads_and_pinned_stays_first(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = open(cx);
    let older = run_shell(cx);
    later(cx);
    let newer = run_shell(cx);
    wait_until(cx, |cx| above(cx, window, &newer, &older));

    click(cx, window, "sidebar-list-options");
    assert_eq!(selected_of(cx, window, "sidebar-order-current"), Some(true));
    click(cx, window, "sidebar-order-attention");
    wait_until(cx, |cx| {
        matches!(
            run_op(
                cx,
                Op::Preference(PreferenceOp::Get {
                    key: kg_domain::prefs::SIDEBAR_ORDER.into(),
                }),
            ),
            OpResult::Setting(value) if value == json!("attention")
        )
    });

    ask(cx, &older);
    assert!(thread_record(cx, &older).unread());
    assert!(
        thread_record(cx, &older).latest_attention_at
            > thread_record(cx, &newer).latest_attention_at
    );
    wait_until(cx, |cx| above(cx, window, &older, &newer));

    set_preference(cx, kg_domain::prefs::SIDEBAR_ORDER, json!("current"));
    wait_until(cx, |cx| above(cx, window, &newer, &older));

    set_preference(cx, kg_domain::prefs::SIDEBAR_UNREAD_FIRST, json!(true));
    wait_until(cx, |cx| above(cx, window, &older, &newer));

    run_op(
        cx,
        Op::Thread(ThreadOp::SetPinned {
            id: newer.clone(),
            pinned: true,
        }),
    );
    cx.run_until_parked();
    wait_until(cx, |cx| above(cx, window, &newer, &older));
}
