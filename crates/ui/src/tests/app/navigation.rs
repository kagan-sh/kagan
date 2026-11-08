use crate::kagan::screen_read;
use gpui_kit::{AnyWindowHandle, AppContext as _, Entity, TestAppContext};
use kg_core::Workspace;
use kg_domain::{InteractionOption, Op, ThreadOp, WindowOp};

use crate::kagan::Kagan;
use crate::tests::kagan::{
    chord, click, open, present, press_key, run_child, run_shell, shown, shows, thread_record,
    wait_until,
};
use crate::tests::sidebar::layout::{focus_row, hover_row};

fn focused(cx: &mut TestAppContext, window: AnyWindowHandle, kagan: &Entity<Kagan>) -> bool {
    cx.update_window(window, |_, window, cx| {
        screen_read::terminal_focused(kagan.read(cx), window, cx)
    })
    .unwrap()
}

fn command(cx: &mut TestAppContext, op: WindowOp) {
    let task = cx.update(|cx| Workspace::dispatch(Op::Window(op), cx));
    cx.foreground_executor().block_test(task).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn thread_shortcuts_follow_visible_order_and_focus_the_terminal(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let mut threads: Vec<_> = (0..10)
        .map(|_| {
            cx.executor()
                .advance_clock(std::time::Duration::from_millis(2));
            run_shell(cx)
        })
        .collect();
    threads.reverse();
    for (index, id) in threads.iter().take(9).enumerate() {
        cx.simulate_keystrokes(window, &chord(&format!("Thread{}", index + 1)));
        cx.run_until_parked();
        assert_eq!(shown(cx, &kagan).as_ref(), Some(id));
        assert!(focused(cx, window, &kagan));
    }
    command(cx, WindowOp::Thread { index: 1 });
    cx.simulate_keystrokes(window, &chord("PreviousThread"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan).as_ref(), threads.last());
    cx.simulate_keystrokes(window, &chord("NextThread"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan).as_ref(), threads.first());
    assert!(focused(cx, window, &kagan));

    let archived = threads.remove(0);
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::Archive {
                id: archived.clone(),
            }),
            cx,
        )
    });
    cx.foreground_executor().block_test(task).unwrap();
    wait_until(cx, |cx| thread_record(cx, &archived).archived_at.is_some());
    command(cx, WindowOp::Thread { index: 1 });
    assert_eq!(shown(cx, &kagan).as_ref(), threads.first());

    focus_row(cx, window, "project-prj_kagan");
    press_key(cx, window, "left");
    cx.run_until_parked();
    command(cx, WindowOp::Thread { index: 1 });
    assert_eq!(shown(cx, &kagan).as_ref(), threads.first());
}

#[gpui_kit::test]
fn next_needing_you_opens_the_oldest_question_in_a_collapsed_project(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let oldest = run_shell(cx);
    let newest = run_shell(cx);
    let current = run_shell(cx);
    for id in [&oldest, &newest, &current] {
        let task = cx.update(|cx| {
            Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx)
        });
        cx.foreground_executor().block_test(task).unwrap();
    }
    click(cx, window, &format!("thread-{current}"));
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    }];
    let first = cx.update(|cx| Workspace::ask(&oldest, "First?".into(), options.clone(), cx));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(2));
    let second = cx.update(|cx| Workspace::ask(&newest, "Second?".into(), options, cx));
    cx.run_until_parked();
    focus_row(cx, window, "project-prj_kagan");
    press_key(cx, window, "left");
    cx.run_until_parked();
    cx.simulate_keystrokes(window, &chord("NextThreadNeedingYou"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan), Some(oldest));
    assert!(focused(cx, window, &kagan));
    cx.simulate_keystrokes(window, &chord("NextThreadNeedingYou"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan), Some(newest));
    drop((first, second));
}

#[gpui_kit::test]
fn navigation_with_no_threads_keeps_the_welcome_screen(cx: &mut TestAppContext) {
    let (kagan, _window, _data) = open(cx);
    for op in [
        WindowOp::NextThread,
        WindowOp::PreviousThread,
        WindowOp::Thread { index: 9 },
        WindowOp::NextThreadNeedingYou,
    ] {
        command(cx, op);
        assert_eq!(shown(cx, &kagan), None);
    }
}

#[gpui_kit::test]
fn next_thread_from_settings_moves_from_the_covered_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    for _ in 0..2 {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(2));
        run_shell(cx);
    }
    cx.simulate_keystrokes(window, &chord("Thread1"));
    cx.run_until_parked();
    let first = shown(cx, &kagan);
    cx.simulate_keystrokes(window, &chord("Thread2"));
    cx.run_until_parked();
    let second = shown(cx, &kagan);
    cx.simulate_keystrokes(window, &chord("Thread1"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan), first);
    click(cx, window, "settings-gear");
    assert_eq!(shown(cx, &kagan), None);
    command(cx, WindowOp::NextThread);
    assert_eq!(shown(cx, &kagan), second);
}

#[gpui_kit::test]
fn needing_you_from_settings_keeps_the_covered_thread(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let asked = run_shell(cx);
    let other = run_shell(cx);
    for id in [&asked, &other] {
        let task = cx.update(|cx| {
            Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx)
        });
        cx.foreground_executor().block_test(task).unwrap();
    }
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    }];
    let question = cx.update(|cx| Workspace::ask(&asked, "Ready?".into(), options, cx));
    cx.run_until_parked();
    click(cx, window, &format!("thread-{asked}"));
    click(cx, window, "settings-gear");
    assert_eq!(shown(cx, &kagan), None);
    command(cx, WindowOp::NextThreadNeedingYou);
    assert_eq!(shown(cx, &kagan), Some(asked));
    drop(question);
}

#[gpui_kit::test]
fn archiving_a_child_moves_to_the_next_row_on_screen(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let tick = |cx: &mut TestAppContext| {
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(2));
    };
    let _oldest = run_shell(cx);
    tick(cx);
    let second = run_shell(cx);
    tick(cx);
    let first = run_shell(cx);
    tick(cx);
    let child = run_child(cx, &first);
    click(cx, window, &format!("thread-{child}"));
    assert_eq!(shown(cx, &kagan), Some(child.clone()));
    cx.simulate_keystrokes(window, &chord("ArchiveThread"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan), Some(second));
}

#[gpui_kit::test]
fn next_needing_you_reveals_a_folded_child(cx: &mut TestAppContext) {
    let (kagan, window, _data) = open(cx);
    let parent = run_shell(cx);
    let child = run_child(cx, &parent);
    let other = run_shell(cx);
    click(cx, window, &format!("thread-{other}"));
    for id in [&parent, &child] {
        let task = cx.update(|cx| {
            Workspace::dispatch(Op::Thread(ThreadOp::MarkRead { id: id.clone() }), cx)
        });
        cx.foreground_executor().block_test(task).unwrap();
    }
    let options = vec![InteractionOption {
        id: "yes".into(),
        label: "Yes".into(),
        role: None,
    }];
    let _asked = cx.update(|cx| Workspace::ask(&child, "Waiting?".into(), options, cx));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan), Some(other));
    hover_row(cx, window, &format!("thread-{parent}"));
    click(cx, window, &format!("thread-disclose-{parent}"));
    cx.run_until_parked();
    assert!(!present(cx, window, &format!("thread-{child}")));
    cx.simulate_keystrokes(window, &chord("NextThreadNeedingYou"));
    cx.run_until_parked();
    assert_eq!(shown(cx, &kagan), Some(child.clone()));
    assert!(shows(cx, window, &format!("thread-{child}")));
}
