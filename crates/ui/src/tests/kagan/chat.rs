use gpui_kit::component::input::Undo;
use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use kg_domain::{
    Event, EventKind, InteractionOption, Link, OptionRole, PendingInteraction,
    PendingInteractionId, Role, Thread, ThreadId, ThreadStatus, Timestamp, ToolKind, ToolStatus,
};

use super::{
    click, disabled, focused_of, label, open, row_top, run_shell, shows, shows_within,
    thread_record, value_of, wait_until,
};
use crate::tests::sidebar::layout::span;

fn seed_thread(data: &std::path::Path, events: Vec<EventKind>) -> ThreadId {
    seed_thread_in(data, None, events)
}

fn seed_thread_in(
    data: &std::path::Path,
    environment: Option<kg_domain::EnvironmentId>,
    events: Vec<EventKind>,
) -> ThreadId {
    let mut store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    if let Some(environment) = &environment {
        store
            .put_environment(&kg_domain::Environment {
                id: environment.clone(),
                project_id: kg_domain::ProjectId("prj_kagan".into()),
                path: data.join("worktree"),
                status: kg_domain::EnvironmentStatus::Ready,
                provider: kg_domain::EnvironmentProvider::Worktree,
                branch: None,
                base_branch: None,
                retire_at: None,
                error: None,
            })
            .unwrap();
    }
    let id = ThreadId("thr_chat".into());
    store
        .put_thread(&Thread {
            id: id.clone(),
            project_id: kg_domain::ProjectId("prj_kagan".into()),
            environment_id: environment,
            title: "Chat".into(),
            title_locked: false,
            provider: "opencode".into(),
            link: Link::Acp,
            status: ThreadStatus::Idle,
            visibility: kg_domain::Visibility::Visible,
            parent_thread_id: None,
            quiet: false,
            notify: kg_domain::ThreadNotify::Default,
            lifecycle_owner_thread_id: None,
            section_id: None,
            pinned: false,
            last_read_at: None,
            latest_attention_at: Timestamp(0),
            mark: kg_domain::ThreadMark::None,
            archived_at: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        })
        .unwrap();
    store.append_events(&id, Timestamp(1), events).unwrap();
    id
}

fn seed_acp_thread(data: &std::path::Path) -> ThreadId {
    seed_thread(data, Vec::new())
}

fn show_chat_with(
    cx: &mut TestAppContext,
    events: Vec<EventKind>,
) -> (
    Entity<crate::kagan::Kagan>,
    AnyWindowHandle,
    tempfile::TempDir,
) {
    let (kagan, window, data) = open(cx);
    let id = seed_thread(data.path(), events);
    reveal(cx, window, &id);
    (kagan, window, data)
}

fn reveal(cx: &mut TestAppContext, window: AnyWindowHandle, id: &ThreadId) {
    // A thread written behind the window's back arrives with the next refresh, which marking it
    // read asks for.
    let read = cx.update(|cx| {
        kg_core::Workspace::dispatch(
            kg_domain::Op::Thread(kg_domain::ThreadOp::MarkRead { id: id.clone() }),
            cx,
        )
    });
    let _ = cx.foreground_executor().block_test(read);
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
}

fn seed_child(
    data: &std::path::Path,
    parent: &ThreadId,
    id: &str,
    title: &str,
    status: ThreadStatus,
) -> ThreadId {
    let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
    let id = ThreadId(id.into());
    store
        .put_thread(&Thread {
            id: id.clone(),
            project_id: kg_domain::ProjectId("prj_kagan".into()),
            environment_id: None,
            title: title.into(),
            title_locked: false,
            provider: "opencode".into(),
            link: Link::Acp,
            status,
            visibility: kg_domain::Visibility::Visible,
            parent_thread_id: Some(parent.clone()),
            quiet: false,
            notify: kg_domain::ThreadNotify::Default,
            lifecycle_owner_thread_id: None,
            section_id: None,
            pinned: false,
            last_read_at: None,
            latest_attention_at: Timestamp(0),
            mark: kg_domain::ThreadMark::None,
            archived_at: None,
            created_at: Timestamp(0),
            updated_at: Timestamp(0),
        })
        .unwrap();
    id
}

fn show_chat(
    cx: &mut TestAppContext,
) -> (
    Entity<crate::kagan::Kagan>,
    AnyWindowHandle,
    tempfile::TempDir,
) {
    show_chat_with(
        cx,
        vec![
            EventKind::Message {
                role: Role::User,
                text: "say hi".into(),
                message_id: None,
                attachments: Vec::new(),
            },
            EventKind::Message {
                role: Role::Agent,
                text: "hi there".into(),
                message_id: None,
                attachments: Vec::new(),
            },
        ],
    )
}

fn tool(
    id: &str,
    kind: ToolKind,
    status: ToolStatus,
    title: &str,
    input: Option<serde_json::Value>,
    output: Option<&str>,
) -> EventKind {
    EventKind::Tool {
        id: id.into(),
        name: None,
        kind: Some(kind),
        title: Some(title.into()),
        status: Some(status),
        input,
        output: output.map(str::to_owned),
        diff: Vec::new(),
        locations: Vec::new(),
        approval: None,
    }
}

fn message(message_id: &str, text: &str) -> EventKind {
    EventKind::Message {
        role: Role::Agent,
        text: text.into(),
        message_id: Some(message_id.into()),
        attachments: Vec::new(),
    }
}

#[gpui_kit::test]
fn an_acp_thread_shows_its_transcript_and_prompt_box(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    assert!(!shows(cx, window, "terminal"));
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-2").is_some_and(|text| text.contains("hi there"))
    });
    assert_eq!(
        value_of(cx, window, "chat-message-1").as_deref(),
        Some("say hi")
    );
    assert!(shows(cx, window, "chat-prompt"));
    assert!(shows(cx, window, "chat-box"));
    // A stopped agent offers the way on where Send would be, and says why in one line.
    assert!(shows(cx, window, "chat-start-again"));
    assert!(!shows(cx, window, "chat-send"));
    assert_eq!(
        value_of(cx, window, "chat-stopped").as_deref(),
        Some("The program is not running.")
    );
    assert!(!shows(cx, window, "ended"), "no second bar under the box");
}

#[gpui_kit::test]
fn a_work_row_expands_and_collapses(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![tool(
            "t1",
            ToolKind::Read,
            ToolStatus::Completed,
            "Read file auth.spec.ts",
            Some(serde_json::json!({"file_path": "auth.spec.ts"})),
            Some("one\ntwo"),
        )],
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-work-1").is_some_and(|value| value.contains("collapsed"))
    });
    assert_eq!(
        value_of(cx, window, "chat-work-1").as_deref(),
        Some("collapsed · done")
    );
    assert!(!shows(cx, window, "chat-work-body-1"));
    click(cx, window, "chat-work-1");
    wait_until(cx, |cx| shows(cx, window, "chat-work-body-1"));
    assert_eq!(
        value_of(cx, window, "chat-work-1").as_deref(),
        Some("expanded · done")
    );
    click(cx, window, "chat-work-1");
    wait_until(cx, |cx| !shows(cx, window, "chat-work-body-1"));
    // Enter does what the click did, once the row has focus.
    for _ in 0..40 {
        cx.simulate_keystrokes(window, "tab");
        cx.run_until_parked();
        if focused_of(cx, window, "chat-work-1") != Some(true) {
            continue;
        }
        cx.simulate_keystrokes(window, "enter");
        cx.run_until_parked();
        break;
    }
    wait_until(cx, |cx| shows(cx, window, "chat-work-body-1"));
}

#[gpui_kit::test]
fn two_open_rows_keep_their_own_bodies(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            tool(
                "t1",
                ToolKind::Execute,
                ToolStatus::Completed,
                "Ran one",
                Some(serde_json::json!({"command": "one"})),
                Some("one"),
            ),
            tool(
                "t2",
                ToolKind::Execute,
                ToolStatus::Completed,
                "Ran two",
                Some(serde_json::json!({"command": "two"})),
                Some("two"),
            ),
        ],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-work-1"));
    click(cx, window, "chat-work-1");
    wait_until(cx, |cx| shows(cx, window, "chat-terminal-1"));
    click(cx, window, "chat-work-2");
    wait_until(cx, |cx| shows(cx, window, "chat-terminal-2"));
    assert!(shows(cx, window, "chat-terminal-1"), "the first body stays");
}

#[gpui_kit::test]
fn a_failed_command_row_reads_failed(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![tool(
            "t1",
            ToolKind::Execute,
            ToolStatus::Failed,
            "Run the tests",
            Some(serde_json::json!({"command": "cargo t"})),
            Some("one failure"),
        )],
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-work-1").is_some_and(|value| value.contains("failed"))
    });
    assert_eq!(
        value_of(cx, window, "chat-work-1").as_deref(),
        Some("collapsed · failed")
    );
    let name = super::label(cx, window, "chat-work-1");
    assert!(name.contains("Ran command"), "{name}");
    assert!(
        name.ends_with("failed"),
        "the row says the word too: {name}"
    );
}

#[gpui_kit::test]
fn a_running_command_previews_its_output(cx: &mut TestAppContext) {
    let lines: Vec<String> = (1..=60).map(|n| format!("line {n}")).collect();
    let output = lines.join("\n");
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![tool(
            "t1",
            ToolKind::Execute,
            ToolStatus::InProgress,
            "Run the tests",
            Some(serde_json::json!({"command": "cargo t"})),
            Some(&output),
        )],
    );
    wait_until(cx, |cx| {
        shows(cx, window, "chat-preview-1")
            && value_of(cx, window, "chat-preview-1").is_some_and(|text| text.contains("line 60"))
    });
    let preview = value_of(cx, window, "chat-preview-1").expect("a preview");
    assert_eq!(preview.lines().count(), 3, "{preview}");
    click(cx, window, "chat-work-1");
    wait_until(cx, |cx| shows(cx, window, "chat-terminal-1"));
    let body = value_of(cx, window, "chat-terminal-1").expect("a body");
    assert!(body.contains("line 60"), "the newest line is in the body");
    assert!(!body.contains("line 1\n"), "the first lines scrolled away");
}

#[gpui_kit::test]
fn a_failure_that_ends_the_turn_reads_expanded(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::TurnStarted,
            tool(
                "t1",
                ToolKind::Execute,
                ToolStatus::Failed,
                "Run the tests",
                Some(serde_json::json!({"command": "cargo t"})),
                Some("one failure"),
            ),
            EventKind::TurnEnded {
                outcome: kg_domain::TurnOutcome::Failed,
                error: None,
            },
        ],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-divider-3"));
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-work-2").as_deref() == Some("expanded · failed")
    });
    assert!(shows(cx, window, "chat-work-body-2"));
}

#[gpui_kit::test]
fn looking_work_folds_into_one_bundle(cx: &mut TestAppContext) {
    let mut events = Vec::new();
    for (id, path) in [("t1", "a.ts"), ("t2", "b.ts"), ("t3", "c.ts")] {
        events.push(tool(
            id,
            ToolKind::Read,
            ToolStatus::Completed,
            "Read file",
            Some(serde_json::json!({"file_path": path})),
            Some("one line"),
        ));
    }
    events.push(tool(
        "t4",
        ToolKind::Edit,
        ToolStatus::Completed,
        "Edited file",
        Some(serde_json::json!({"file_path": "a.ts"})),
        None,
    ));
    let (_kagan, window, _data) = show_chat_with(cx, events);
    wait_until(cx, |cx| shows(cx, window, "chat-bundle-1"));
    assert_eq!(
        super::label(cx, window, "chat-bundle-1"),
        "Explored 3 files"
    );
    assert!(shows(cx, window, "chat-work-4"), "the edit is its own row");
    assert!(!shows(cx, window, "chat-work-1"), "a bundle hides its rows");
    click(cx, window, "chat-bundle-1");
    for id in 1..=3 {
        let row = format!("chat-work-{id}");
        wait_until(cx, |cx| shows(cx, window, &row));
    }
    // A member opens on its own, and the bundle stays open around it.
    click(cx, window, "chat-work-1");
    wait_until(cx, |cx| shows(cx, window, "chat-output-1"));
    assert_eq!(
        value_of(cx, window, "chat-work-1").as_deref(),
        Some("expanded · done")
    );
    assert_eq!(
        value_of(cx, window, "chat-bundle-1").as_deref(),
        Some("expanded · done")
    );
}

#[gpui_kit::test]
fn a_bundle_closes_when_the_turn_ends(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::TurnStarted,
            tool(
                "t1",
                ToolKind::Read,
                ToolStatus::Completed,
                "Read file",
                Some(serde_json::json!({"file_path": "a.ts"})),
                Some("one line"),
            ),
            tool(
                "t2",
                ToolKind::Read,
                ToolStatus::Completed,
                "Read file",
                Some(serde_json::json!({"file_path": "b.ts"})),
                Some("one line"),
            ),
            EventKind::TurnEnded {
                outcome: kg_domain::TurnOutcome::Completed,
                error: None,
            },
        ],
    );
    // The turn started at seq 1, the reads folded at 2, and the divider is seq 4.
    wait_until(cx, |cx| shows(cx, window, "chat-bundle-2"));
    assert_eq!(
        value_of(cx, window, "chat-bundle-2").as_deref(),
        Some("collapsed · done")
    );
    assert!(shows(cx, window, "chat-divider-4"));
    assert_eq!(
        value_of(cx, window, "chat-divider-4").as_deref(),
        Some("Worked for 1 s")
    );
}

#[gpui_kit::test]
fn a_plan_row_is_collapsed_once_the_turn_ends(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::TurnStarted,
            EventKind::Plan {
                entries: vec![kg_domain::PlanEntry {
                    text: "read the tests".into(),
                    status: kg_domain::PlanStatus::Pending,
                }],
            },
            EventKind::TurnEnded {
                outcome: kg_domain::TurnOutcome::Completed,
                error: None,
            },
        ],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-plan-2"));
    assert_eq!(
        value_of(cx, window, "chat-plan-2").as_deref(),
        Some("collapsed · done")
    );
}

fn show_approval(
    cx: &mut TestAppContext,
    kind: ToolKind,
    options: Vec<InteractionOption>,
    environment: Option<kg_domain::EnvironmentId>,
) -> (AnyWindowHandle, tempfile::TempDir) {
    let (_kagan, window, data) = open(cx);
    let asked = PendingInteraction {
        id: PendingInteractionId("pn_1".into()),
        thread_id: ThreadId("thr_chat".into()),
        title: "opencode wants to run a command".into(),
        options,
        fields: None,
        created_at: Timestamp(1),
    };
    let id = seed_thread_in(
        data.path(),
        environment,
        vec![
            tool(
                "t1",
                kind,
                ToolStatus::InProgress,
                "Run the tests",
                Some(serde_json::json!({"command": "cargo t"})),
                None,
            ),
            EventKind::Tool {
                id: "t1".into(),
                name: None,
                kind: Some(kind),
                title: Some("Run the tests".into()),
                status: Some(ToolStatus::Pending),
                input: Some(serde_json::json!({"command": "cargo t"})),
                output: None,
                diff: Vec::new(),
                locations: Vec::new(),
                approval: Some(asked.id.clone()),
            },
        ],
    );
    let store = kg_store::Store::open(data.path().join("kagan.sqlite3")).unwrap();
    store.put_pending_interaction(&asked).unwrap();
    drop(store);
    let read = cx.update(|cx| {
        kg_core::Workspace::dispatch(
            kg_domain::Op::Thread(kg_domain::ThreadOp::MarkRead { id: id.clone() }),
            cx,
        )
    });
    let _ = cx.foreground_executor().block_test(read);
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| shows(cx, window, "chat-transcript"));
    wait_until(cx, |cx| shows(cx, window, "chat-approval-pn_1"));
    (window, data)
}

fn permission_options() -> Vec<InteractionOption> {
    vec![
        InteractionOption {
            id: "reject-once".into(),
            label: "Deny".into(),
            role: Some(OptionRole::Destructive),
        },
        InteractionOption {
            id: "allow-once".into(),
            label: "Allow once".into(),
            role: Some(OptionRole::Primary),
        },
    ]
}

fn tool_with(
    id: &str,
    name: &str,
    kind: ToolKind,
    title: &str,
    input: serde_json::Value,
) -> EventKind {
    EventKind::Tool {
        id: id.into(),
        name: Some(name.into()),
        kind: Some(kind),
        title: Some(title.into()),
        status: Some(ToolStatus::Completed),
        input: Some(input),
        output: Some("one".into()),
        diff: Vec::new(),
        locations: Vec::new(),
        approval: None,
    }
}

#[gpui_kit::test]
fn a_body_leads_to_where_it_points(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::Tool {
                id: "t1".into(),
                name: Some("mcp__github__list_issues".into()),
                kind: Some(ToolKind::Other),
                title: Some("List issues".into()),
                status: Some(ToolStatus::Completed),
                input: Some(serde_json::json!({"state": "open"})),
                output: Some(r#"[{"number":1},{"number":2}]"#.into()),
                diff: Vec::new(),
                locations: Vec::new(),
                approval: None,
            },
            tool_with(
                "t2",
                "skill_load",
                ToolKind::Other,
                "Load a skill",
                serde_json::json!({"name": "rust-review"}),
            ),
            EventKind::Tool {
                id: "t3".into(),
                name: None,
                kind: Some(ToolKind::Edit),
                title: Some("Edited file".into()),
                status: Some(ToolStatus::Completed),
                input: Some(serde_json::json!({"file_path": "a.ts"})),
                output: None,
                diff: vec![kg_domain::ToolDiff {
                    path: "a.ts".into(),
                    old: Some("one".into()),
                    new: "two".into(),
                }],
                locations: Vec::new(),
                approval: None,
            },
        ],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-work-1"));
    click(cx, window, "chat-work-1");
    wait_until(cx, |cx| {
        super::label(cx, window, "chat-items-1").as_str() == "2 items"
    });
    click(cx, window, "chat-work-2");
    wait_until(cx, |cx| shows(cx, window, "chat-link-skill-2"));
    click(cx, window, "chat-work-3");
    wait_until(cx, |cx| shows(cx, window, "chat-link-review-3"));
}

#[gpui_kit::test]
fn an_mcp_row_names_its_tool_once(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![EventKind::Tool {
            id: "t1".into(),
            name: Some("mcp__github__list_issues".into()),
            kind: Some(ToolKind::Other),
            title: Some("List issues".into()),
            status: Some(ToolStatus::Completed),
            input: Some(serde_json::json!({"state": "open"})),
            output: None,
            diff: Vec::new(),
            locations: Vec::new(),
            approval: None,
        }],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-work-1"));
    assert_eq!(
        super::label(cx, window, "chat-work-1"),
        "Called github.list_issues List issues"
    );
}

#[gpui_kit::test]
fn the_approval_card_answers_through_the_question(cx: &mut TestAppContext) {
    let (window, _data) = show_approval(cx, ToolKind::Execute, permission_options(), None);
    wait_until(cx, |cx| shows(cx, window, "answer-chat-pn_1-allow-once"));
    wait_until(cx, |cx| shows(cx, window, "answer-chat-pn_1-reject-once"));
    // A command that is waiting for permission has not started: it says so, with its own mark.
    assert_eq!(
        value_of(cx, window, "chat-work-1").as_deref(),
        Some("collapsed · waiting")
    );
    let name = label(cx, window, "chat-work-1");
    assert!(name.contains("Waiting to run command"), "{name}");
    assert!(!name.contains("Running command"), "{name}");
    assert!(
        !shows(cx, window, "chat-approval-runs-pn_1"),
        "a thread with no folder of its own says nothing about a worktree"
    );
}

#[gpui_kit::test]
fn the_approval_card_lists_the_reject_first_and_the_filled_option_last(cx: &mut TestAppContext) {
    let mut options = permission_options();
    options.pop();
    let agent_order = vec![
        InteractionOption {
            id: "allow-once".into(),
            label: "Allow once".into(),
            role: Some(OptionRole::Primary),
        },
        InteractionOption {
            id: "allow-always".into(),
            label: "Always allow".into(),
            role: None,
        },
        options.remove(0),
    ];
    let (window, _data) = show_approval(cx, ToolKind::Execute, agent_order, None);
    wait_until(cx, |cx| {
        shows_within(
            cx,
            window,
            "answer-chat-pn_1-allow-always",
            "chat-approval-pn_1",
        )
    });
    let left = |cx: &mut TestAppContext, id: &str| {
        cx.update_window(window, |_, window, _| {
            crate::ui_registry::find(window, &ElementId::Name(id.into()))
                .map(|element| element.bounds.left())
        })
        .unwrap()
    };
    let deny = left(cx, "answer-chat-pn_1-reject-once").expect("the deny button");
    let always = left(cx, "answer-chat-pn_1-allow-always").expect("the always allow button");
    let once = left(cx, "answer-chat-pn_1-allow-once").expect("the allow once button");
    assert!(
        deny < always && always < once,
        "{deny:?} {always:?} {once:?}"
    );
}

#[gpui_kit::test]
fn a_command_in_a_worktree_says_where_it_runs(cx: &mut TestAppContext) {
    let (window, _data) = show_approval(
        cx,
        ToolKind::Execute,
        permission_options(),
        Some(kg_domain::EnvironmentId("env_1".into())),
    );
    wait_until(cx, |cx| shows(cx, window, "chat-approval-runs-pn_1"));
    assert!(shows(cx, window, "chat-approval-value-pn_1"));
}

#[gpui_kit::test]
fn the_transcript_follows_the_tail_and_offers_jump_to_latest(cx: &mut TestAppContext) {
    let events: Vec<EventKind> = (0..60)
        .map(|index| message(&format!("m{index}"), &format!("answer {index}")))
        .collect();
    let (_kagan, window, _data) = show_chat_with(cx, events);
    wait_until(cx, |cx| shows(cx, window, "chat-message-60"));
    // A wheel scroll up takes the list away from its tail.
    cx.update_window(window, |_, window, cx| {
        crate::ui_dispatch::scroll_within(
            window,
            &ElementId::Name("chat-message-60".into()),
            None,
            gpui_kit::ScrollDelta::Pixels(gpui_kit::point(gpui_kit::px(0.), gpui_kit::px(1200.))),
            cx,
        )
        .expect("the transcript scrolls");
    })
    .unwrap();
    wait_until(cx, |cx| !shows(cx, window, "chat-message-60"));
    // The list is scrolled up, so the built-in Jump to latest button sits over the bottom of the
    // transcript. It is gpui-kit's own button, so it is clicked by its place, not an element id.
    cx.update_window(window, |_, window, cx| {
        let transcript = ElementId::Name("chat-transcript".into());
        let bounds = crate::ui_registry::find(window, &transcript)
            .expect("the transcript")
            .bounds;
        crate::ui_dispatch::click_at(
            window,
            gpui_kit::point(bounds.center().x, bounds.bottom() - gpui_kit::px(24.)),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "chat-message-60"));
}

#[gpui_kit::test]
fn escape_with_no_reply_open_says_nothing(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    cx.dispatch_action(window, crate::kagan::CancelTurn);
    cx.run_until_parked();
    assert!(!shows(cx, window, "note"));
}

#[gpui_kit::test]
fn a_message_is_named_by_who_spoke_and_when(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-2").is_some_and(|text| text.contains("hi there"))
    });
    let agent = label(cx, window, "chat-message-2");
    assert!(
        agent.starts_with("OpenCode, "),
        "the program's short name and the time: {agent}"
    );
    assert!(!agent.contains("hi there"), "{agent}");
    let user = label(cx, window, "chat-message-1");
    assert!(user.starts_with("You, "), "{user}");
}

fn emit(cx: &mut TestAppContext, event: Event) {
    cx.update(|cx| {
        kg_core::Workspace::global(cx).update(cx, |_, cx| {
            cx.emit(kg_core::WorkspaceEvent::Event {
                event: Box::new(event),
                project_id: kg_domain::ProjectId("prj_kagan".into()),
            });
        });
    });
}

fn agent_said(id: &ThreadId, seq: u64, text: &str) -> Event {
    Event {
        thread_id: id.clone(),
        seq,
        at: Timestamp(2),
        kind: EventKind::Message {
            role: Role::Agent,
            text: text.into(),
            message_id: None,
            attachments: Vec::new(),
        },
    }
}

#[gpui_kit::test]
fn an_event_the_load_already_read_is_drawn_once(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    let id = ThreadId("thr_chat".into());
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-2").is_some_and(|text| text.contains("hi there"))
    });
    emit(cx, agent_said(&id, 2, "hi there"));
    assert_eq!(
        value_of(cx, window, "chat-message-2").as_deref(),
        Some("hi there"),
        "the chunk the load already read is not written again"
    );
    emit(
        cx,
        Event {
            thread_id: id.clone(),
            seq: 3,
            at: Timestamp(3),
            kind: EventKind::Notice {
                severity: kg_domain::NoticeSeverity::Info,
                text: "and again".into(),
            },
        },
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-notice-3").is_some_and(|text| text.contains("and again"))
    });
}

#[gpui_kit::test]
fn a_load_that_fails_still_draws_the_events_after_it(cx: &mut TestAppContext) {
    let (kagan, _window, data) = open(cx);
    // The store holds this thread and the workspace has never heard of it, so its load fails.
    let id = seed_acp_thread(data.path());
    let view = kagan.downgrade();
    let window = cx
        .add_window(|window, cx| crate::kagan::chat::ChatView::new(id.clone(), view, window, cx))
        .into();
    cx.run_until_parked();
    assert!(!shows(cx, window, "chat-message-1"), "nothing was read");
    emit(cx, agent_said(&id, 1, "the first word"));
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-message-1").as_deref() == Some("the first word")
    });
}

#[gpui_kit::test]
fn a_rewritten_draft_undoes_with_the_normal_key(cx: &mut TestAppContext) {
    let (kagan, _window, _data) = open(cx);
    // A running thread, so the box takes typing; the view is drawn on its own so it has the key.
    let id = run_shell(cx);
    let handle = cx.add_window(|window, cx| {
        crate::kagan::chat::ChatView::new(id.clone(), kagan.downgrade(), window, cx)
    });
    let chat = handle.root(cx).unwrap();
    let window: AnyWindowHandle = handle.into();
    wait_until(cx, |cx| shows(cx, window, "chat-prompt"));
    assert_eq!(disabled(cx, window, "chat-prompt"), Some(false));
    click(cx, window, "chat-prompt");
    wait_until(cx, |cx| focused_of(cx, window, "chat-prompt") == Some(true));
    cx.update_window(window, |_, window, cx| {
        chat.update(cx, |chat, cx| {
            chat.set_draft("improve: ".into(), window, cx);
        });
    })
    .unwrap();
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-prompt").as_deref() == Some("improve: ")
    });
    cx.dispatch_action(window, Undo);
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-prompt").as_deref() == Some("")
    });
}

#[gpui_kit::test]
fn the_prompt_box_is_one_box_in_the_transcripts_column(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    let (column_x, column_w) = span(cx, window, "chat-column").expect("the transcript column");
    let (box_x, box_w) = span(cx, window, "chat-box").expect("the box");
    assert!(
        (column_x - box_x).abs() <= 0.5,
        "the box starts where the transcript does: {box_x} against {column_x}"
    );
    assert!(
        (column_w - box_w).abs() <= 0.5,
        "the box is as wide as the transcript column: {box_w} against {column_w}"
    );
    let (prompt_x, prompt_w) = span(cx, window, "chat-prompt").expect("the text area");
    assert!(prompt_x >= box_x && prompt_x + prompt_w <= box_x + box_w);
    let (action_x, action_w) = span(cx, window, "chat-start-again").expect("the action");
    assert!(
        action_x >= box_x && action_x + action_w <= box_x + box_w,
        "Send, Cancel and Start again sit inside the box"
    );
    assert!(
        row_top(cx, window, "chat-context") > row_top(cx, window, "chat-start-again"),
        "the context row sits under the box"
    );
}

#[gpui_kit::test]
fn an_empty_transcript_centers_one_quiet_line_in_the_column(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(cx, Vec::new());
    wait_until(cx, |cx| shows(cx, window, "chat-empty"));
    let (empty_x, empty_w) = span(cx, window, "chat-empty").expect("the empty line");
    let (box_x, box_w) = span(cx, window, "chat-box").expect("the box");
    let (empty_mid, box_mid) = (empty_x + empty_w / 2., box_x + box_w / 2.);
    assert!(
        (empty_mid - box_mid).abs() <= 1.,
        "the empty line is centered over the box: {empty_mid} against {box_mid}"
    );
}

#[gpui_kit::test]
fn a_chat_that_exited_says_it_once_and_offers_start_again(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::Started,
            EventKind::Exited {
                code: Some(0),
                signal: None,
            },
        ],
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-stopped").as_deref() == Some("Exited with code 0.")
    });
    assert!(shows(cx, window, "chat-start-again"));
    assert!(!shows(cx, window, "chat-send"), "one action, not two");
    assert!(
        !shows(cx, window, "ended"),
        "no second bar carries the same news"
    );
}

#[gpui_kit::test]
fn a_stopped_chat_takes_no_typing_in_its_prompt(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::Started,
            EventKind::Exited {
                code: Some(0),
                signal: None,
            },
        ],
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-stopped").as_deref() == Some("Exited with code 0.")
    });
    assert_eq!(disabled(cx, window, "chat-prompt"), Some(true));
    click(cx, window, "chat-prompt");
    assert_eq!(
        focused_of(cx, window, "chat-prompt"),
        Some(true),
        "the box takes focus, and still takes no typing"
    );
    cx.simulate_keystrokes(window, "x");
    cx.run_until_parked();
    assert_eq!(
        value_of(cx, window, "chat-prompt").as_deref(),
        Some(""),
        "a stopped box keeps no typing"
    );
}

#[gpui_kit::test]
fn the_context_row_names_the_project_and_the_folder(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    assert_eq!(
        value_of(cx, window, "chat-context-project").as_deref(),
        Some("Kagan project")
    );
    assert_eq!(
        value_of(cx, window, "chat-context-folder").as_deref(),
        Some("Project folder")
    );
    assert_eq!(
        value_of(cx, window, "chat-state").as_deref(),
        Some("Not running")
    );
}

#[gpui_kit::test]
fn a_worktree_thread_names_its_own_folder(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let id = seed_thread_in(
        data.path(),
        Some(kg_domain::EnvironmentId("env_kagan".into())),
        Vec::new(),
    );
    reveal(cx, window, &id);
    assert_eq!(
        value_of(cx, window, "chat-context-folder").as_deref(),
        Some("worktree")
    );
}

#[gpui_kit::test]
fn the_plan_card_is_one_line_until_it_is_opened(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![EventKind::Plan {
            entries: vec![
                kg_domain::PlanEntry {
                    text: "Reproduce it".into(),
                    status: kg_domain::PlanStatus::Completed,
                },
                kg_domain::PlanEntry {
                    text: "Fix it".into(),
                    status: kg_domain::PlanStatus::Pending,
                },
            ],
        }],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-card-plan"));
    assert_eq!(
        value_of(cx, window, "chat-card-plan").as_deref(),
        Some("collapsed, 1 of 2 done")
    );
    assert!(!shows(cx, window, "chat-card-plan-body"));
    click(cx, window, "chat-card-plan");
    wait_until(cx, |cx| shows(cx, window, "chat-card-plan-body"));
    assert_eq!(
        value_of(cx, window, "chat-card-plan").as_deref(),
        Some("expanded, 1 of 2 done")
    );
}

#[gpui_kit::test]
fn the_child_threads_card_counts_the_ones_that_run(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let parent = seed_thread(data.path(), Vec::new());
    let child = seed_child(
        data.path(),
        &parent,
        "thr_child_run",
        "Review the migration",
        ThreadStatus::Active,
    );
    seed_child(
        data.path(),
        &parent,
        "thr_child_idle",
        "An older one",
        ThreadStatus::Idle,
    );
    reveal(cx, window, &parent);
    wait_until(cx, |cx| shows(cx, window, "chat-card-children"));
    assert_eq!(
        value_of(cx, window, "chat-card-children").as_deref(),
        Some("collapsed, 1")
    );
    assert!(!shows(cx, window, "chat-card-children-body"));
    click(cx, window, "chat-card-children");
    wait_until(cx, |cx| shows(cx, window, "chat-card-children-body"));
    assert!(shows(cx, window, &format!("chat-card-child-{}", child.0)));
}

#[gpui_kit::test]
fn an_archived_chat_offers_no_way_on(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let id = seed_thread_in(
        data.path(),
        Some(kg_domain::EnvironmentId("env_kagan".into())),
        vec![
            EventKind::Started,
            EventKind::Exited {
                code: Some(0),
                signal: None,
            },
        ],
    );
    reveal(cx, window, &id);
    wait_until(cx, |cx| shows(cx, window, "chat-start-again"));
    assert!(
        shows(cx, window, "chat-more"),
        "a stopped chat keeps its quiet ways behind one menu"
    );
    cx.dispatch_action(window, crate::kagan::ArchiveThread);
    wait_until(cx, |cx| thread_record(cx, &id).archived_at.is_some());
    // An archived row is only reachable from the archived list, and opening it shows the chat
    // again, read only.
    click(cx, window, "archived");
    wait_until(cx, |cx| shows(cx, window, &format!("thread-{}", id.0)));
    click(cx, window, &format!("thread-{}", id.0));
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-stopped").as_deref() == Some("This thread is archived.")
    });
    assert!(!shows(cx, window, "chat-start-again"));
    assert!(
        !shows(cx, window, "chat-more"),
        "an archived chat offers no way on"
    );
}

#[gpui_kit::test]
fn a_stopped_chats_more_menu_opens_the_start_screen_and_saves_diagnostics(cx: &mut TestAppContext) {
    let (_kagan, window, data) = open(cx);
    let id = seed_thread_in(
        data.path(),
        Some(kg_domain::EnvironmentId("env_kagan".into())),
        vec![
            EventKind::Started,
            EventKind::Failed {
                error: Some("no such file or directory".into()),
            },
        ],
    );
    reveal(cx, window, &id);
    wait_until(cx, |cx| shows(cx, window, "chat-more"));
    click(cx, window, "chat-more");
    wait_until(cx, |cx| shows(cx, window, "chat-new-thread"));
    assert!(shows(cx, window, "chat-save-diagnostics"));
    click(cx, window, "chat-new-thread");
    wait_until(cx, |cx| shows(cx, window, "reuse-hint"));
    super::press_key(cx, window, "escape");
    wait_until(cx, |cx| shows(cx, window, "chat-more"));

    click(cx, window, "chat-more");
    wait_until(cx, |cx| shows(cx, window, "chat-save-diagnostics"));
    click(cx, window, "chat-save-diagnostics");
    let folder = data.path().join("diagnostics");
    wait_until(cx, |_cx| {
        std::fs::read_dir(&folder).is_ok_and(|entries| {
            entries
                .filter_map(Result::ok)
                .any(|entry| entry.path().extension().is_some_and(|ext| ext == "txt"))
        })
    });
}

#[gpui_kit::test]
fn a_stopped_chat_with_nothing_quiet_to_offer_draws_no_more_menu(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::Started,
            EventKind::Exited {
                code: Some(0),
                signal: None,
            },
        ],
    );
    assert!(shows(cx, window, "chat-start-again"));
    assert!(!shows(cx, window, "chat-more"));
}

#[gpui_kit::test]
fn a_chat_that_failed_says_why(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![
            EventKind::Started,
            EventKind::Failed {
                error: Some("no such file or directory".into()),
            },
        ],
    );
    wait_until(cx, |cx| {
        value_of(cx, window, "chat-stopped").as_deref() == Some("no such file or directory")
    });
}

#[gpui_kit::test]
fn a_card_opens_and_closes_from_the_keyboard(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat_with(
        cx,
        vec![EventKind::Plan {
            entries: vec![kg_domain::PlanEntry {
                text: "Reproduce it".into(),
                status: kg_domain::PlanStatus::Pending,
            }],
        }],
    );
    wait_until(cx, |cx| shows(cx, window, "chat-card-plan"));
    // A click opens the card and gives it the keyboard; Enter closes it, and Space opens it again.
    click(cx, window, "chat-card-plan");
    wait_until(cx, |cx| shows(cx, window, "chat-card-plan-body"));
    cx.simulate_keystrokes(window, "enter");
    cx.run_until_parked();
    wait_until(cx, |cx| !shows(cx, window, "chat-card-plan-body"));
    cx.simulate_keystrokes(window, "space");
    cx.run_until_parked();
    wait_until(cx, |cx| shows(cx, window, "chat-card-plan-body"));
}
