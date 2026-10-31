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
}

#[gpui_kit::test]
fn a_plain_title_waits_only_after_one_second(cx: &mut gpui_kit::TestAppContext) {
    let thread = cx.update(|cx| cx.new(|_| crate::Thread::new(running())));
    let now = std::time::Instant::now();
    cx.update(|cx| {
        assert_eq!(
            super::follow(&thread, Some("\u{2819} work"), now, cx),
            Some(true)
        );
        assert_eq!(super::follow(&thread, Some("work"), now, cx), None);
        assert!(thread.read(cx).busy(), "the title just went plain");
        assert_eq!(
            super::follow(
                &thread,
                Some("work"),
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
                now + std::time::Duration::from_secs(1),
                cx,
            ),
            Some(false)
        );
        assert!(!thread.read(cx).busy());
    });
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
        model: String::new(),
        reasoning_level: String::new(),
        status: ThreadStatus::Active,
        visibility: Visibility::Visible,
        parent_thread_id: None,
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
