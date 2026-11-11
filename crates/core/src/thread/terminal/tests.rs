use gpui_kit::AppContext;

use super::busy;

// Claude 2.1.308 shows U+25D0 and U+25D1. U+25D2 and U+25D3 are in the same half-circle range and are absent from that capture.
#[test]
fn claude_titles_with_a_half_circle_first_are_busy() {
    assert!(busy("\u{25d0} Claude Code"));
    assert!(busy("\u{25d1} Counting from 1 to 25"));
    assert!(busy("\u{25d2} Counting from 1 to 25"));
    assert!(busy("\u{25d3} Counting from 1 to 25"));
}

// Claude 2.1.308 writes U+2733 when idle.
#[test]
fn claude_idle_title_is_not_busy() {
    assert!(!busy("\u{2733} Claude Code"));
    assert!(!busy("\u{2733} Counting from 1 to 25"));
}

// These titles are from a codex 0.163.0 capture.
#[test]
fn codex_titles_with_a_braille_frame_first_are_busy() {
    assert!(busy("\u{2819} codex-dir"));
    assert!(busy("\u{280f} codex-dir"));
}

// These titles are from a codex 0.163.0 capture.
#[test]
fn codex_idle_titles_are_not_busy() {
    assert!(!busy("codex-dir"));
    assert!(!busy("Count to 40 | codex-dir"));
}

// Claude before 2.1.228 uses a braille spinner. U+280B is absent from the 2.1.308 capture.
#[test]
fn a_braille_frame_first_is_busy_for_older_claude() {
    assert!(busy("\u{280b} project"));
}

#[test]
fn a_spinner_that_is_not_first_is_not_busy() {
    assert!(!busy("project \u{2819}"));
    assert!(!busy(""));
    assert!(!busy("project \u{23f3} Working"));
}

#[gpui_kit::test]
fn a_plain_title_waits_only_after_one_second(cx: &mut gpui_kit::TestAppContext) {
    let thread = cx.update(|cx| cx.new(|_| crate::Thread::new(running())));
    let now = std::time::Instant::now();
    cx.update(|cx| {
        assert!(matches!(
            super::follow(&thread, Some("\u{2819} work"), false, || 0, now, cx),
            Some(super::Spin::Opened)
        ));
        assert!(thread.read(cx).turn.open());
        assert_eq!(
            super::follow(&thread, Some("work"), false, || 0, now, cx),
            None
        );
        assert!(thread.read(cx).busy(), "the title just went plain");
        assert!(thread.read(cx).turn.open());
        assert_eq!(
            super::follow(
                &thread,
                Some("work"),
                false,
                || 0,
                now + std::time::Duration::from_millis(999),
                cx,
            ),
            None
        );
        assert!(thread.read(cx).busy(), "one second has not passed");
        assert_eq!(
            super::follow(
                &thread,
                Some("work"),
                false,
                || 0,
                now + std::time::Duration::from_secs(1),
                cx,
            ),
            Some(super::Spin::Closed)
        );
        assert!(!thread.read(cx).busy());
        assert!(!thread.read(cx).turn.open());
    });
}

#[gpui_kit::test]
fn a_spinner_after_stop_clears_busy_does_not_open_another_turn(cx: &mut gpui_kit::TestAppContext) {
    let thread = cx.update(|cx| cx.new(|_| crate::Thread::new(running())));
    let now = std::time::Instant::now();
    cx.update(|cx| {
        assert!(matches!(
            super::follow(&thread, Some("\u{2819} work"), false, || 0, now, cx),
            Some(super::Spin::Opened)
        ));
        thread.update(cx, |thread, _| thread.busy = false);
        assert!(matches!(
            super::follow(&thread, Some("\u{2819} work"), false, || 0, now, cx),
            Some(super::Spin::Resumed)
        ));
        assert!(thread.read(cx).busy());
        assert!(thread.read(cx).turn.open());
    });
}

#[test]
fn a_start_failure_names_the_program_and_folder() {
    let message = super::start_failure(
        std::path::Path::new("tools/claude"),
        std::path::Path::new("work/folder"),
        &std::io::Error::from(std::io::ErrorKind::NotFound),
    );
    assert!(message.contains("tools/claude"), "{message}");
    assert!(message.contains("work/folder"), "{message}");
}

fn running() -> kg_domain::Thread {
    use kg_domain::{ProjectId, Thread, ThreadId, ThreadStatus, Timestamp, Visibility};
    let at = Timestamp(0);
    Thread {
        id: ThreadId("thr_quiet".into()),
        project_id: ProjectId("prj".into()),
        environment_id: None,
        title: "claude".into(),
        title_locked: false,
        provider: "claude".into(),
        status: ThreadStatus::Active,
        visibility: Visibility::Visible,
        parent_thread_id: None,
        quiet: false,
        notify: kg_domain::ThreadNotify::Default,
        lifecycle_owner_thread_id: None,
        section_id: None,
        pinned: false,
        last_read_at: Some(at),
        latest_attention_at: at,
        mark: kg_domain::ThreadMark::None,
        archived_at: None,
        created_at: at,
        updated_at: at,
    }
}

#[gpui_kit::test]
fn a_hook_turn_is_not_closed_by_a_plain_title_from_an_earlier_turn(
    cx: &mut gpui_kit::TestAppContext,
) {
    use crate::thread::turn_state::{Signal, Source, TurnState};
    let thread = cx.update(|cx| cx.new(|_| crate::Thread::new(running())));
    let first = std::time::Instant::now();
    let later = first + std::time::Duration::from_secs(60);
    cx.update(|cx| {
        thread.update(cx, |thread, _| thread.turn = TurnState::new(Source::Hook));
        thread.update(cx, |thread, _| thread.turn.apply(Signal::HookStart, first));
        assert_ne!(
            super::follow(&thread, Some("work"), false, || 0, first, cx),
            Some(super::Spin::Closed)
        );
        thread.update(cx, |thread, _| thread.turn.apply(Signal::HookEnd, first));
        thread.update(cx, |thread, _| thread.turn.apply(Signal::HookStart, later));
        assert_ne!(
            super::follow(&thread, Some("work"), false, || 0, later, cx),
            Some(super::Spin::Closed),
            "a second hook turn closed on its first tick"
        );
        assert!(thread.read(cx).turn.open());
    });
}
