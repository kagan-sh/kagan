use gpui_kit::{AnyWindowHandle, AppContext as _, ElementId, Entity, TestAppContext};
use kg_domain::{
    EventKind, InteractionOption, Link, OptionRole, PendingInteraction, PendingInteractionId, Role,
    Thread, ThreadId, ThreadStatus, Timestamp, ToolKind, ToolStatus,
};

use super::{click, focused_of, open, shows, shows_within, value_of, wait_until};

/// A thread the store already holds: an ACP one has no program in this process, so the center
/// shows its transcript and prompt box.
fn seed_thread(data: &std::path::Path, events: Vec<EventKind>) -> ThreadId {
    seed_thread_in(data, None, events)
}

/// The same, in a folder of the thread's own.
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

/// Opens a window on a seeded ACP thread and shows its chat.
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
    (kagan, window, data)
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

/// One ACP tool delta, as the agent's update becomes it.
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
    assert!(shows(cx, window, "chat-send"));
    // The hint says what this state wants, and `kg ui` can read it.
    assert_eq!(
        value_of(cx, window, "chat-hint").as_deref(),
        Some("The agent is not running. Start it again below.")
    );
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

/// Two open rows draw two bodies, each with an id of its own, so neither hides the other.
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

/// A running command shows its newest lines under the row, without opening it, and its body keeps
/// the newest lines too.
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

/// A failure that ends the turn stays open, past the divider the turn ends with.
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

/// Work is collapsed by default: a plan row opens only while it is a live turn's frontier.
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

/// A thread whose running command asks the person, and the card it draws.
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

/// A body leads to the place it talks about, and an MCP answer says how much it carries.
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
fn the_approval_card_answers_through_the_question(cx: &mut TestAppContext) {
    let (window, _data) = show_approval(cx, ToolKind::Execute, permission_options(), None);
    wait_until(cx, |cx| shows(cx, window, "answer-chat-pn_1-allow-once"));
    wait_until(cx, |cx| shows(cx, window, "answer-chat-pn_1-reject-once"));
    assert_eq!(
        value_of(cx, window, "chat-work-1").as_deref(),
        Some("collapsed · busy")
    );
    assert!(
        !shows(cx, window, "chat-approval-runs-pn_1"),
        "a thread with no folder of its own says nothing about a worktree"
    );
}

/// The agent sends its options in its own order; the card reads Deny, then Always allow, then the
/// filled Allow once.
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

/// Escape with no reply open is left for whatever else handles it: a stopped thread says nothing.
#[gpui_kit::test]
fn escape_with_no_reply_open_says_nothing(cx: &mut TestAppContext) {
    let (_kagan, window, _data) = show_chat(cx);
    cx.dispatch_action(window, crate::kagan::CancelTurn);
    cx.run_until_parked();
    assert!(!shows(cx, window, "note"));
}
