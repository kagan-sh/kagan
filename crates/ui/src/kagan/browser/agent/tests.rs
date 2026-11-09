use super::{Agent, idle};
use crate::kagan::Kagan;
use crate::kagan::browser::page_url;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{OpError, ThreadId};
use std::time::{Duration, Instant};

const IDLE_ENDED: &str = "the browser session ended after 30 idle minutes";

fn thread(name: &str) -> ThreadId {
    ThreadId(name.to_owned())
}

fn with_sh(cx: &mut TestAppContext) {
    let path = std::env::var_os("PATH").unwrap_or_default();
    cx.update(|cx| {
        Workspace::global(cx).update(cx, |workspace, _| {
            workspace.use_program_environment(
                std::path::Path::new(&path),
                std::path::Path::new("/bin/sh"),
            );
        });
    });
}

/// An open window and one thread the sidebar knows, for the drive tests.
fn harness(cx: &mut TestAppContext) -> (Entity<Kagan>, AnyWindowHandle, ThreadId) {
    let (kagan, window, _data) = crate::tests::kagan::open(cx);
    with_sh(cx);
    let owner = crate::tests::kagan::run_shell(cx);
    (kagan, window, owner)
}

fn stale(now: Instant) -> Instant {
    now.checked_sub(Duration::from_mins(31))
        .expect("the test clock is past the epoch plus 31 minutes")
}

#[test]
fn thirty_idle_minutes_end_the_drive() {
    let used = Instant::now();
    assert!(!idle(used, used + Duration::from_mins(29)));
    assert!(idle(used, used + Duration::from_mins(31)));
}

#[gpui_kit::test]
fn an_idle_drive_ends_before_the_next_command(cx: &mut TestAppContext) {
    let (kagan, window, owner) = harness(cx);
    let now = Instant::now();
    for begin in [false, true] {
        let result = cx
            .update_window(window, |_, window, cx| {
                kagan.update(cx, |kagan, cx| {
                    kagan.browser.agent = Some(Agent {
                        thread: owner.clone(),
                        url: "https://example.com/".into(),
                        used: stale(now),
                    });
                    kagan.browser.page = None;
                    if begin {
                        kagan
                            .begin_open(
                                &owner,
                                &page_url("example.com").expect("a URL"),
                                now,
                                window,
                                cx,
                            )
                            .map(|_| ())
                    } else {
                        kagan.require(&owner, now, window, cx).map(|_| ())
                    }
                })
            })
            .expect("a window");
        assert_eq!(
            result,
            Err(OpError::NotFound(IDLE_ENDED.into())),
            "begin={begin}"
        );
        assert!(
            cx.update(|cx| kagan.read(cx).browser.agent.is_none()),
            "the drive ended, begin={begin}"
        );
    }
}

#[gpui_kit::test]
fn another_thread_is_refused_with_the_owners_name(cx: &mut TestAppContext) {
    let (kagan, window, owner) = harness(cx);
    let caller = thread("thr_other");
    let title = cx
        .update(|cx| {
            kagan
                .read(cx)
                .record(&owner, cx)
                .map(|thread| thread.title.clone())
        })
        .expect("the owner thread");
    let refused = OpError::Invalid(format!(
        "thread {title} is driving the browser; close it with kg browser close"
    ));
    let now = Instant::now();
    for begin in [false, true] {
        let result = cx
            .update_window(window, |_, window, cx| {
                kagan.update(cx, |kagan, cx| {
                    kagan.browser.agent = Some(Agent {
                        thread: owner.clone(),
                        url: String::new(),
                        used: now,
                    });
                    kagan.browser.page = None;
                    if begin {
                        kagan
                            .begin_open(
                                &caller,
                                &page_url("example.com").expect("a URL"),
                                now,
                                window,
                                cx,
                            )
                            .map(|_| ())
                    } else {
                        kagan.require(&caller, now, window, cx).map(|_| ())
                    }
                })
            })
            .expect("a window");
        assert_eq!(result, Err(refused.clone()), "begin={begin}");
        assert!(
            cx.update(|cx| kagan.read(cx).browser.agent.is_some()),
            "the owner keeps the drive, begin={begin}"
        );
    }
}
