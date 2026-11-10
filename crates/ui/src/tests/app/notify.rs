use std::path::Path;
use std::time::{Duration, Instant};

use gpui_kit::{AppContext as _, SystemNotification, SystemNotificationResponse, TestAppContext};
use kg_core::Workspace;
use kg_domain::{
    EnvironmentSpec, NotifyOp, Op, OpResult, ProjectId, Run, ThreadId, ThreadNotify, ThreadOp,
};

use crate::tests::kagan::{
    app_then, host, open_with_plugins, set_preference, shown, thread_record,
};

const SPINNER: &str = "printf '\\033]2;\\342\\227\\220 work\\007'";
const PLAIN: &str = "printf '\\033]2;\\342\\234\\263 work\\007'";

#[gpui_kit::test]
fn a_quiet_thread_notifies_when_no_window_shows_it(cx: &mut TestAppContext) {
    let data = host(cx);
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    let notes = until_notice(cx);
    stop(cx, &id);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].title.as_ref(), thread_record(cx, &id).title);
    assert_eq!(notes[0].tag.as_ref(), format!("kagan:thread:{id}"));
}

#[gpui_kit::test]
fn clicking_the_notice_opens_that_thread(cx: &mut TestAppContext) {
    let (kagan, _window, data) = open_with_plugins(cx, false);
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    let [note] = until_notice(cx).try_into().unwrap();
    cx.simulate_system_notification_response(SystemNotificationResponse {
        tag: note.tag,
        action_id: None,
    });
    cx.run_until_parked();
    stop(cx, &id);
    assert_eq!(shown(cx, &kagan), Some(id));
}

#[gpui_kit::test]
fn a_notice_click_opens_its_thread_before_the_plugin_host_starts(cx: &mut TestAppContext) {
    let data = app_then(cx, |_| {});
    let kagan = cx.update(|cx| crate::kagan::open(cx).unwrap());
    cx.run_until_parked();
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    let [note] = until_notice(cx).try_into().unwrap();
    cx.simulate_system_notification_response(SystemNotificationResponse {
        tag: note.tag,
        action_id: None,
    });
    cx.run_until_parked();
    stop(cx, &id);
    assert_eq!(shown(cx, &kagan), Some(id));
}

#[gpui_kit::test]
fn a_thread_on_screen_is_marked_read_and_not_notified(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins(cx, false);
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 2; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.select(id.clone(), window, cx));
    })
    .unwrap();
    let deadline = Instant::now() + Duration::from_secs(8);
    let mut saw_busy = false;
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let (busy, _) = mark(cx, &id);
        saw_busy |= busy;
        if saw_busy && !busy {
            break;
        }
        assert!(Instant::now() < deadline, "the thread stayed busy");
        std::thread::sleep(Duration::from_millis(20));
    }
    stop(cx, &id);
    assert!(cx.delivered_system_notifications().is_empty());
    assert!(!thread_record(cx, &id).unread());
}

#[gpui_kit::test]
fn a_second_spinner_frame_does_not_notify(cx: &mut TestAppContext) {
    let data = host(cx);
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; {SPINNER}; sleep 30"),
    );
    let id = start(cx, &program);
    park(cx, Duration::from_millis(2500));
    stop(cx, &id);
    assert!(cx.delivered_system_notifications().is_empty());
}

#[gpui_kit::test]
fn notify_off_suppresses_the_next_one(cx: &mut TestAppContext) {
    let data = host(cx);
    set_preference(cx, "notify", serde_json::json!("off"));
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    park(cx, Duration::from_millis(2500));
    stop(cx, &id);
    assert!(cx.delivered_system_notifications().is_empty());
}

#[gpui_kit::test]
fn stop_a_failed_start_and_a_plain_title_do_not_notify(cx: &mut TestAppContext) {
    let data = host(cx);
    let sleeping = script(data.path(), "sleeping", "sleep 30");
    let id = start(cx, &sleeping);
    stop(cx, &id);
    let plain = script(
        data.path(),
        "plain",
        "printf '\\033]2;plain\\007'; sleep 30",
    );
    let id = start(cx, &plain);
    park(cx, Duration::from_millis(1500));
    stop(cx, &id);
    let missing = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Run(Run {
                project_id: ProjectId("prj_kagan".into()),
                program: "definitely-missing-kg-bin".into(),
                title: None,
                environment: EnvironmentSpec::Personal,
                visibility: None,
                parent_thread_id: None,
                quiet: false,
                lifecycle_owner_thread_id: None,
                metadata: None,
            })),
            cx,
        )
    });
    assert!(cx.foreground_executor().block_test(missing).is_err());
    assert!(cx.delivered_system_notifications().is_empty());
}

#[gpui_kit::test]
fn asks_only_skips_a_turn_that_ends(cx: &mut TestAppContext) {
    let data = host(cx);
    set_preference(cx, "notify", serde_json::json!("asks"));
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    park(cx, Duration::from_millis(2500));
    stop(cx, &id);
    assert!(cx.delivered_system_notifications().is_empty());
}

#[gpui_kit::test]
fn a_muted_thread_posts_nothing(cx: &mut TestAppContext) {
    let data = host(cx);
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    set_thread_notify(cx, &id, ThreadNotify::Muted);
    park(cx, Duration::from_millis(2500));
    stop(cx, &id);
    assert!(cx.delivered_system_notifications().is_empty());
}

#[gpui_kit::test]
fn a_thread_level_all_posts_while_the_global_level_is_off(cx: &mut TestAppContext) {
    let data = host(cx);
    set_preference(cx, "notify", serde_json::json!("off"));
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    set_thread_notify(cx, &id, ThreadNotify::All);
    let notes = until_notice(cx);
    stop(cx, &id);
    assert_eq!(notes.len(), 1);
}

#[gpui_kit::test]
fn opening_a_thread_clears_its_notice(cx: &mut TestAppContext) {
    let (kagan, window, data) = open_with_plugins(cx, false);
    let program = script(
        data.path(),
        "spin",
        &format!("{SPINNER}; sleep 1; {PLAIN}; sleep 30"),
    );
    let id = start(cx, &program);
    let [note] = until_notice(cx).try_into().unwrap();
    cx.update_window(window, |_, window, cx| {
        kagan.update(cx, |kagan, cx| kagan.select(id.clone(), window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    stop(cx, &id);
    assert!(
        cx.dismissed_system_notifications()
            .iter()
            .any(|tag| tag.as_ref() == note.tag.as_ref())
    );
}

#[gpui_kit::test]
fn the_test_notification_posts_even_when_notify_is_off(cx: &mut TestAppContext) {
    let _data = host(cx);
    set_preference(cx, "notify", serde_json::json!("off"));
    let task = cx.update(|cx| Workspace::dispatch(Op::Notify(NotifyOp::Test), cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
    let notes = cx.delivered_system_notifications();
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].tag.as_ref(), "kagan:test");
}

fn set_thread_notify(cx: &mut TestAppContext, id: &ThreadId, level: ThreadNotify) {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Notify {
                id: id.clone(),
                level,
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

fn script(dir: &Path, name: &str, body: &str) -> String {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path.display().to_string()
}

fn start(cx: &mut TestAppContext, program: &str) -> ThreadId {
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Run(Run {
                project_id: ProjectId("prj_kagan".into()),
                program: program.into(),
                title: None,
                environment: EnvironmentSpec::Personal,
                visibility: None,
                parent_thread_id: None,
                quiet: false,
                lifecycle_owner_thread_id: None,
                metadata: None,
            })),
            cx,
        )
    });
    let OpResult::Thread(thread) = cx.foreground_executor().block_test(task).unwrap() else {
        panic!("thread");
    };
    cx.run_until_parked();
    thread.id
}

fn stop(cx: &mut TestAppContext, id: &ThreadId) {
    let task =
        cx.update(|cx| Workspace::dispatch(Op::Thread(ThreadOp::Stop { id: id.clone() }), cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

fn until_notice(cx: &mut TestAppContext) -> Vec<SystemNotification> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        let notes = cx.delivered_system_notifications();
        if !notes.is_empty() || Instant::now() >= deadline {
            return notes;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn park(cx: &mut TestAppContext, total: Duration) {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        cx.executor().advance_clock(Duration::from_millis(50));
        cx.run_until_parked();
        std::thread::sleep(Duration::from_millis(20));
    }
}

fn mark(cx: &mut TestAppContext, id: &ThreadId) -> (bool, bool) {
    cx.update(|cx| {
        let thread = Workspace::global(cx).read(cx).thread(id).unwrap();
        let thread = thread.read(cx);
        (thread.busy(), thread.record().unread())
    })
}
