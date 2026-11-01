use crate::{
    AgentTool, ConfigOp, DiffNote, DiffScope, DiffSide, EnvironmentSpec, Event, EventKind,
    InteractionOption, Op, OpError, OpResult, PendingInteraction, PendingInteractionId, PluginId,
    PluginOp, Thread, ThreadId, ThreadOp, ThreadStatus, Timestamp, TurnOutcome, Visibility,
    branch_slug, format_notes, invalid_json, parse_notes,
};
use serde_json::{Value, json};

fn round_trip<T>(value: &T, wire: &Value)
where
    T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug,
{
    assert_eq!(&serde_json::to_value(value).unwrap(), wire);
    assert_eq!(&serde_json::from_value::<T>(wire.clone()).unwrap(), value);
}

#[test]
fn run_op() {
    let op = Op::Thread(ThreadOp::Run(crate::Run {
        project_id: crate::ProjectId("proj".into()),
        program: "claude".into(),
        title: None,
        environment: EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        },
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }));
    let wire = serde_json::to_value(&op).unwrap();
    assert_eq!(wire["thread"]["run"]["program"], "claude");
    assert_eq!(serde_json::from_value::<Op>(wire).unwrap(), op);
}

#[test]
fn discover_programs_round_trips() {
    round_trip(
        &Op::Program(crate::ProgramOp::Discover),
        &json!({"program": "discover"}),
    );
}

#[test]
fn unit_op() {
    round_trip(&Op::Sidebar, &json!("sidebar"));
}

#[test]
fn omitted_options_read_as_none() {
    let op: Op = serde_json::from_value(json!({ "thread": { "list": {} } })).unwrap();
    assert_eq!(
        op,
        Op::Thread(ThreadOp::List {
            project_id: None,
            archived: None,
            limit: None,
            before: None,
        })
    );
}

#[test]
fn environment_spec() {
    round_trip(
        &EnvironmentSpec::Worktree {
            base_branch: Some("build".into()),
            branch: None,
        },
        &json!({ "kind": "worktree", "baseBranch": "build" }),
    );
    round_trip(
        &EnvironmentSpec::ProjectDefault,
        &json!({ "kind": "project-default" }),
    );
}

#[test]
fn turn_ended_event() {
    round_trip(
        &Event {
            thread_id: ThreadId("thr_1".into()),
            seq: 7,
            at: Timestamp(1_000),
            kind: EventKind::TurnEnded {
                outcome: TurnOutcome::Interrupted,
                error: None,
            },
        },
        &json!({
            "threadId": "thr_1",
            "seq": 7,
            "at": 1000,
            "type": "turn.ended",
            "data": { "outcome": "interrupted", "error": null },
        }),
    );
}

#[test]
fn bare_event_has_no_data() {
    round_trip(
        &Event {
            thread_id: ThreadId("thr_1".into()),
            seq: 1,
            at: Timestamp(0),
            kind: EventKind::Idle,
        },
        &json!({ "threadId": "thr_1", "seq": 1, "at": 0, "type": "idle" }),
    );
}

#[test]
fn a_branch_slug_comes_from_the_title() {
    assert_eq!(branch_slug("Review PR #1936"), "review-pr-1936");
    assert_eq!(branch_slug("..."), "");
}

#[test]
fn thread_unread_when_attention_is_newer_than_read() {
    let wire = json!({
        "id": "thr_1", "projectId": "prj_1", "environmentId": null, "title": "t", "titleLocked": false,
        "provider": "claude-code", "model": "opus", "reasoningLevel": "high",
        "status": "idle", "visibility": "visible",
        "parentThreadId": null, "lifecycleOwnerThreadId": null, "sectionId": null,
        "pinned": false, "lastReadAt": 5, "latestAttentionAt": 5, "mark": "none", "archivedAt": null,
        "createdAt": 1, "updatedAt": 5,
    });
    let mut thread: Thread = serde_json::from_value(wire.clone()).unwrap();
    round_trip(&thread, &wire);
    let mut legacy = wire.clone();
    legacy.as_object_mut().unwrap().remove("titleLocked");
    legacy.as_object_mut().unwrap().remove("mark");
    assert!(
        !serde_json::from_value::<Thread>(legacy)
            .unwrap()
            .title_locked
    );
    assert!(!thread.unread());
    thread.latest_attention_at = Timestamp(6);
    assert!(thread.unread());
    thread.last_read_at = None;
    assert!(thread.unread());
}

#[test]
fn wait_event_op() {
    round_trip(
        &Op::Thread(ThreadOp::WaitEvent {
            id: ThreadId("thr_1".into()),
            kind: "turn.ended".into(),
            after_seq: Some(3),
            timeout_ms: 5_000,
        }),
        &json!({ "thread": { "wait-event": {
            "id": "thr_1", "kind": "turn.ended", "afterSeq": 3,             "timeoutMs": 5000,
        }}}),
    );
}

#[test]
fn ask_op() {
    round_trip(
        &Op::Thread(ThreadOp::Ask {
            id: ThreadId("thr_1".into()),
            title: "Ship it?".into(),
            options: vec![
                InteractionOption {
                    id: "yes".into(),
                    label: "Yes".into(),
                },
                InteractionOption {
                    id: "no".into(),
                    label: "No".into(),
                },
            ],
        }),
        &json!({ "thread": { "ask": {
            "id": "thr_1",
            "title": "Ship it?",
            "options": [
                { "id": "yes", "label": "Yes" },
                { "id": "no", "label": "No" },
            ],
        }}}),
    );
}

#[test]
fn wait_op() {
    round_trip(
        &Op::Thread(ThreadOp::Wait {
            id: ThreadId("thr_1".into()),
            timeout_ms: 5_000,
        }),
        &json!({ "thread": { "wait": {
            "id": "thr_1", "timeoutMs": 5000,
        }}}),
    );
}

#[test]
fn error_wire() {
    let error = OpError::NotFound("thread thr_1 not found".into());
    let wire = json!({ "error": { "kind": "not-found", "message": "thread thr_1 not found" } });
    assert_eq!(serde_json::to_value(&error).unwrap(), wire);
    assert_eq!(serde_json::from_value::<OpError>(wire).unwrap(), error);
    let hinted = json!({ "error": {
        "kind": "not-found",
        "message": "thread thr_1 not found",
        "hint": "set KG_THREAD_ID, or pass the id",
    }});
    assert_eq!(serde_json::from_value::<OpError>(hinted).unwrap(), error);
    assert!(
        serde_json::from_value::<OpError>(json!({ "error": { "kind": "odd", "message": "" } }))
            .is_err()
    );
    let plain = serde_json::to_value(OpError::Invalid("nope".into())).unwrap();
    assert!(plain["error"].get("hint").is_none());
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&invalid_json("nope", "")).unwrap(),
        plain
    );
    let with_hint = serde_json::from_str::<serde_json::Value>(&invalid_json(
        "nope",
        "set KG_THREAD_ID, or pass the id",
    ))
    .unwrap();
    assert_eq!(
        with_hint["error"]["hint"],
        "set KG_THREAD_ID, or pass the id"
    );
    assert!(with_hint.get("ok").is_none());
    let done = serde_json::to_value(OpResult::Done).unwrap();
    assert!(done.get("ok").is_none());
    assert!(done.get("hint").is_none());
}

#[test]
fn plugin_use_op() {
    round_trip(
        &Op::Plugin(PluginOp::Use {
            slot: crate::Slot::Review,
            id: crate::SlotChoice::Kagan,
            thread_id: None,
        }),
        &json!({ "plugin": { "use": { "slot": "review", "id": "kagan", "threadId": null }}}),
    );
}

#[test]
fn a_slot_choice_is_kagan_or_a_plugin_id() {
    use crate::SlotChoice;
    assert_eq!("kagan".parse::<SlotChoice>(), Ok(SlotChoice::Kagan));
    assert_eq!(
        "dev.example.list".parse::<SlotChoice>(),
        Ok(SlotChoice::Plugin(PluginId("dev.example.list".into())))
    );
    for bad in ["../../outside", "a.b-", "single", "Dev.Example", ""] {
        assert!(bad.parse::<SlotChoice>().is_err(), "{bad}");
    }
}

#[test]
fn a_config_value_never_shows_in_a_debug_print() {
    let op = Op::Plugin(PluginOp::Config {
        id: PluginId("dev.kagan.linear".into()),
        op: ConfigOp::Set {
            key: "token".into(),
            value: Some(json!("lin_api_debug")),
        },
        thread_id: None,
    });
    let printed = format!("{op:?}");
    assert!(
        printed.contains("token") && printed.contains("<hidden>"),
        "{printed}"
    );
    assert!(!printed.contains("lin_api_debug"), "{printed}");
}

#[test]
fn plugin_config_op() {
    round_trip(
        &Op::Plugin(PluginOp::Config {
            id: PluginId("dev.kagan.vocodr".into()),
            op: ConfigOp::Set {
                key: "sounds".into(),
                value: Some(json!(false)),
            },
            thread_id: Some(ThreadId("thr_1".into())),
        }),
        &json!({ "plugin": { "config": {
            "id": "dev.kagan.vocodr", "op": { "set": { "key": "sounds", "value": false } },
            "threadId": "thr_1",
        }}}),
    );
}

#[test]
fn tool_results() {
    round_trip(
        &OpResult::Tools {
            instructions: None,
            tools: vec![AgentTool {
                name: "editor_open".into(),
                description: "Open a file".into(),
                input_schema: json!({ "type": "object" }),
            }],
        },
        &json!({ "tools": { "instructions": null, "tools": [
            { "name": "editor_open", "description": "Open a file", "inputSchema": { "type": "object" } },
        ]}}),
    );
    round_trip(
        &OpResult::ToolReply {
            text: "opened".into(),
            is_error: false,
        },
        &json!({ "tool-reply": { "text": "opened", "isError": false } }),
    );
}

#[test]
fn answers_are_not_operations() {
    let value = json!({ "thread": { "answer": {
        "id": "thr_1", "interactionId": "int_1", "optionId": "allow",
    }}});
    assert!(serde_json::from_value::<Op>(value).is_err());
}

#[test]
fn interactions_result() {
    round_trip(
        &OpResult::Interactions(vec![PendingInteraction {
            id: PendingInteractionId("int_1".into()),
            thread_id: ThreadId("thr_1".into()),
            title: "Run cargo test?".into(),
            options: vec![InteractionOption {
                id: "allow-once".into(),
                label: "Allow once".into(),
            }],
            created_at: Timestamp(5),
        }]),
        &json!({ "interactions": [{
            "id": "int_1",
            "threadId": "thr_1",
            "title": "Run cargo test?",
            "options": [{ "id": "allow-once", "label": "Allow once" }],
            "createdAt": 5,
        }]}),
    );
}

#[test]
fn theme_settings_are_typed_and_theme_ids_cannot_escape_the_catalog() {
    use crate::{Appearance, ThemeOp, ThemeSettings, valid_theme_id};
    round_trip(
        &Op::Theme(ThemeOp::Set {
            theme: "ocean".into(),
            mode: Appearance::Light,
        }),
        &json!({ "theme": { "set": { "theme": "ocean", "mode": "light" } } }),
    );
    round_trip(
        &OpResult::Theme(ThemeSettings {
            theme: "fjord".into(),
            mode: Appearance::Dark,
        }),
        &json!({ "theme": { "theme": "fjord", "mode": "dark" } }),
    );
    assert!(valid_theme_id("ocean-2"));
    for id in ["", "../ocean", "a/b", "a\\b", "Ocean", ".", "ocean.json"] {
        assert!(!valid_theme_id(id), "{id}");
    }
    assert!(
        serde_json::from_value::<ThemeSettings>(json!({"theme":"fjord","mode":"sepia"})).is_err()
    );
}

#[test]
fn theme_cli_parses_modes_and_rejects_paths() {
    use clap::Parser;
    #[derive(Parser)]
    struct Cli {
        #[command(subcommand)]
        op: Op,
    }
    assert_eq!(
        Cli::try_parse_from(["kg", "theme", "set", "ocean", "light"])
            .unwrap()
            .op,
        Op::Theme(crate::ThemeOp::Set {
            theme: "ocean".into(),
            mode: crate::Appearance::Light
        })
    );
    assert!(Cli::try_parse_from(["kg", "theme", "set", "../ocean", "light"]).is_err());
    assert!(Cli::try_parse_from(["kg", "theme", "set", "ocean", "sepia"]).is_err());
    assert_eq!(
        Cli::try_parse_from(["kg", "theme", "set", "dev.kagan.example/midnight", "dark"])
            .unwrap()
            .op,
        Op::Theme(crate::ThemeOp::Set {
            theme: "dev.kagan.example/midnight".into(),
            mode: crate::Appearance::Dark
        })
    );
    assert_eq!(
        Cli::try_parse_from([
            "kg",
            "theme",
            "new",
            "mine",
            "--from",
            "dev.kagan.example/midnight"
        ])
        .unwrap()
        .op,
        Op::Theme(crate::ThemeOp::New {
            id: "mine".into(),
            from: Some("dev.kagan.example/midnight".into()),
        })
    );
    assert!(Cli::try_parse_from(["kg", "theme", "new", "dev.kagan.example/mine"]).is_err());
}

#[test]
fn a_theme_color_holds_only_rgb() {
    let color = crate::Color::new(0x1ff_ffff);
    assert_eq!(color.rgb(), 0xff_ffff);
    assert_eq!(color.hex(), "#ffffff");
}

#[test]
fn a_plugin_theme_is_named_by_its_plugin() {
    for id in ["ocean", "dev.kagan.example/midnight", "a.b/c-2"] {
        assert!(crate::valid_theme_choice(id), "{id}");
    }
    for id in [
        "",
        "example/midnight",
        "dev.kagan.example/",
        "/midnight",
        "dev..x/m",
        "a.b/c/d",
        "a.b/../c",
        "Dev.x/m",
    ] {
        assert!(!crate::valid_theme_choice(id), "{id}");
    }
}

#[test]
fn event_type_names_match_the_wire_tag() {
    use crate::{Role, ToolStatus};
    let kinds = [
        EventKind::Created,
        EventKind::Started,
        EventKind::Exited {
            code: None,
            signal: None,
        },
        EventKind::Stopped,
        EventKind::Idle,
        EventKind::Waiting,
        EventKind::Failed { error: None },
        EventKind::Archived,
        EventKind::Unarchived,
        EventKind::Deleted,
        EventKind::TurnStarted,
        EventKind::TurnEnded {
            outcome: TurnOutcome::Completed,
            error: None,
        },
        EventKind::Message {
            role: Role::Agent,
            text: String::new(),
            message_id: None,
            attachments: Vec::new(),
        },
        EventKind::Tool {
            id: String::new(),
            name: None,
            status: ToolStatus::Completed,
        },
        EventKind::RestoreFailed {
            error: String::new(),
        },
        EventKind::SnapshotFailed {
            error: String::new(),
        },
        EventKind::Notice {
            severity: crate::NoticeSeverity::Warning,
            text: String::new(),
        },
        EventKind::Answered {
            title: String::new(),
            option: String::new(),
        },
        EventKind::FileChanged {
            path: String::new(),
        },
        EventKind::FileRead {
            path: String::new(),
        },
        EventKind::FilePredicted {
            path: String::new(),
            line: String::new(),
        },
        EventKind::Opened,
        EventKind::Rewound { before_seq: 1 },
        EventKind::Reconnecting {
            text: String::new(),
        },
    ];
    for (kind, name) in kinds.iter().zip(EventKind::TYPES) {
        assert_eq!(kind.type_name(), name);
        assert_eq!(serde_json::to_value(kind).unwrap()["type"], name);
    }
    assert_eq!(kinds.len(), EventKind::TYPES.len());
}

#[test]
fn reset_drops_the_saved_shortcut_and_ignores_old_platform_maps() {
    use crate::Keymap;
    let mut keymap = Keymap::from_setting(Some(json!({
        "shared": { "ToggleSidebar": "secondary-x" },
        "mac": { "ToggleSidebar": "secondary-m" },
        "windows": { "NewThread": "ctrl-n" },
        "linux": { "NewThread": "ctrl-n" }
    })))
    .unwrap();
    assert_eq!(keymap.binding("ToggleSidebar"), Some("secondary-x"));
    assert!(keymap.binding("NewThread").is_none());
    keymap.reset("ToggleSidebar");
    assert!(keymap.binding("ToggleSidebar").is_none());
}

#[test]
fn every_platform_keeps_the_same_effective_bindings() {
    use crate::{KeyPlatform, Keymap, SHORTCUTS, effective_chord};
    let keymap = Keymap::default();
    let listed = SHORTCUTS
        .iter()
        .map(|shortcut| {
            let mac = effective_chord(shortcut.name, &keymap, KeyPlatform::Mac);
            let windows = effective_chord(shortcut.name, &keymap, KeyPlatform::Windows);
            let linux = effective_chord(shortcut.name, &keymap, KeyPlatform::Linux);
            assert_eq!(windows, linux, "{}", shortcut.name);
            if let Some(rest) = shortcut.chord.strip_prefix("secondary-") {
                assert_eq!(mac, shortcut.chord, "{}", shortcut.name);
                assert_eq!(
                    windows,
                    format!("ctrl-shift-{}", rest.trim_start_matches("shift-")),
                    "{}",
                    shortcut.name
                );
            } else {
                assert_eq!(mac, shortcut.chord, "{}", shortcut.name);
                assert_eq!(windows, shortcut.chord, "{}", shortcut.name);
            }
            format!("{}\t{mac}\t{windows}\t{linux}", shortcut.name)
        })
        .collect::<Vec<_>>()
        .join("\n");
    assert_eq!(
        listed,
        "\
Bigger\tsecondary-=\tctrl-shift-=\tctrl-shift-=
Smaller\tsecondary--\tctrl-shift--\tctrl-shift--
ActualSize\tsecondary-0\tctrl-shift-0\tctrl-shift-0
NewThread\tsecondary-n\tctrl-shift-n\tctrl-shift-n
StopThread\tsecondary-.\tctrl-shift-.\tctrl-shift-.
ArchiveThread\tsecondary-shift-backspace\tctrl-shift-backspace\tctrl-shift-backspace
TogglePinned\tsecondary-shift-p\tctrl-shift-p\tctrl-shift-p
NextThread\tsecondary-shift-]\tctrl-shift-]\tctrl-shift-]
PreviousThread\tsecondary-shift-[\tctrl-shift-[\tctrl-shift-[
NextThreadNeedingYou\tsecondary-alt-j\tctrl-shift-alt-j\tctrl-shift-alt-j
Thread1\tsecondary-1\tctrl-shift-1\tctrl-shift-1
Thread2\tsecondary-2\tctrl-shift-2\tctrl-shift-2
Thread3\tsecondary-3\tctrl-shift-3\tctrl-shift-3
Thread4\tsecondary-4\tctrl-shift-4\tctrl-shift-4
Thread5\tsecondary-5\tctrl-shift-5\tctrl-shift-5
Thread6\tsecondary-6\tctrl-shift-6\tctrl-shift-6
Thread7\tsecondary-7\tctrl-shift-7\tctrl-shift-7
Thread8\tsecondary-8\tctrl-shift-8\tctrl-shift-8
Thread9\tsecondary-9\tctrl-shift-9\tctrl-shift-9
OpenSettings\tsecondary-,\tctrl-shift-,\tctrl-shift-,
UseProjectFolder\tsecondary-alt-p\tctrl-shift-alt-p\tctrl-shift-alt-p
UseWorktreeFolder\tsecondary-alt-w\tctrl-shift-alt-w\tctrl-shift-alt-w
RetryBranches\tsecondary-alt-r\tctrl-shift-alt-r\tctrl-shift-alt-r
RetryPrograms\tsecondary-alt-l\tctrl-shift-alt-l\tctrl-shift-alt-l
ToggleCustomCommand\tsecondary-alt-c\tctrl-shift-alt-c\tctrl-shift-alt-c
ThreadActions\tsecondary-shift-m\tctrl-shift-m\tctrl-shift-m
ToggleSidebar\tsecondary-b\tctrl-shift-b\tctrl-shift-b
ToggleRightSidebar\tsecondary-\\\tctrl-shift-\\\tctrl-shift-\\
AddProject\tsecondary-shift-o\tctrl-shift-o\tctrl-shift-o
SendReview\tsecondary-enter\tctrl-shift-enter\tctrl-shift-enter
NextHunk\talt-]\talt-]\talt-]
PrevHunk\talt-[\talt-[\talt-[
FindInDiff\tsecondary-f\tctrl-shift-f\tctrl-shift-f
ToggleWrap\talt-z\talt-z\talt-z
NextMatch\tsecondary-g\tctrl-shift-g\tctrl-shift-g
PrevMatch\tsecondary-shift-g\tctrl-shift-g\tctrl-shift-g
StartAgain\tsecondary-shift-s\tctrl-shift-s\tctrl-shift-s
FocusSidebar\tsecondary-shift-l\tctrl-shift-l\tctrl-shift-l
RestoreThread\tsecondary-shift-r\tctrl-shift-r\tctrl-shift-r
UseBuiltInSidebar\tsecondary-alt-b\tctrl-shift-alt-b\tctrl-shift-alt-b
UseBuiltInReview\tsecondary-alt-v\tctrl-shift-alt-v\tctrl-shift-alt-v
OpenCommandList\tsecondary-shift-a\tctrl-shift-a\tctrl-shift-a
SearchThreads\tsecondary-alt-f\tctrl-shift-alt-f\tctrl-shift-alt-f
NewTerminal\tsecondary-t\tctrl-shift-t\tctrl-shift-t
CloseTerminal\tsecondary-shift-w\tctrl-shift-w\tctrl-shift-w
NextTerminal\tsecondary-alt-]\tctrl-shift-alt-]\tctrl-shift-alt-]
PreviousTerminal\tsecondary-alt-[\tctrl-shift-alt-[\tctrl-shift-alt-[
OpenLocalLink\tsecondary-alt-o\tctrl-shift-alt-o\tctrl-shift-alt-o"
    );
}

#[test]
fn archive_span_counts_a_visible_thread_under_a_hidden_or_archived_owner() {
    use crate::archive_span;
    let owner = sample("thr_owner", None, Visibility::Visible, None);
    let dependent = sample(
        "thr_dep",
        Some(ThreadId("thr_owner".into())),
        Visibility::Visible,
        None,
    );
    let hidden = sample(
        "thr_hidden",
        Some(ThreadId("thr_owner".into())),
        Visibility::Hidden,
        None,
    );
    let under_hidden = sample(
        "thr_under_hidden",
        Some(ThreadId("thr_hidden".into())),
        Visibility::Visible,
        None,
    );
    let archived = sample(
        "thr_archived",
        Some(ThreadId("thr_owner".into())),
        Visibility::Visible,
        Some(Timestamp(1)),
    );
    let under_archived = sample(
        "thr_under_archived",
        Some(ThreadId("thr_archived".into())),
        Visibility::Visible,
        None,
    );
    let child = {
        let mut thread = sample("thr_child", None, Visibility::Visible, None);
        thread.parent_thread_id = Some(ThreadId("thr_owner".into()));
        thread
    };
    let threads = [
        owner,
        dependent,
        hidden,
        under_hidden,
        archived,
        under_archived,
        child,
    ];
    assert_eq!(archive_span(&threads, &ThreadId("thr_owner".into())), 3);
}

fn sample(
    id: &str,
    owner: Option<ThreadId>,
    visibility: Visibility,
    archived_at: Option<Timestamp>,
) -> Thread {
    Thread {
        id: ThreadId(id.into()),
        project_id: crate::ProjectId("proj".into()),
        environment_id: None,
        title: id.into(),
        title_locked: false,
        provider: String::new(),
        model: String::new(),
        reasoning_level: String::new(),
        status: ThreadStatus::Idle,
        visibility,
        parent_thread_id: None,
        lifecycle_owner_thread_id: owner,
        section_id: None,
        pinned: false,
        last_read_at: None,
        latest_attention_at: Timestamp(0),
        mark: crate::ThreadMark::None,
        archived_at,
        created_at: Timestamp(0),
        updated_at: Timestamp(0),
    }
}

#[test]
fn a_stop_rejects_a_thread_id_with_a_space_and_a_run_has_none() {
    let stop = Op::Thread(ThreadOp::Stop {
        id: ThreadId("not plain".into()),
    });
    assert_eq!(
        crate::check(&stop),
        Err("thread id must be a plain identifier")
    );
    let run = Op::Thread(ThreadOp::Run(crate::Run {
        project_id: crate::ProjectId("project".into()),
        program: "sh".into(),
        title: None,
        environment: EnvironmentSpec::Worktree {
            base_branch: None,
            branch: None,
        },
        visibility: None,
        parent_thread_id: None,
        lifecycle_owner_thread_id: None,
        metadata: None,
    }));
    assert_eq!(crate::check(&run), Ok(()));
}

#[test]
fn notes_round_trip() {
    let notes = vec![
        DiffNote {
            scope: DiffScope::Commit,
            rev: Some("abc123".into()),
            path: "src/a.rs".into(),
            side: DiffSide::New,
            start: 12,
            end: 14,
            text: "Please rename this.\nIt is the public name.".into(),
        },
        DiffNote {
            scope: DiffScope::Uncommitted,
            rev: None,
            path: "src/b.rs".into(),
            side: DiffSide::New,
            start: 3,
            end: 3,
            text: "Why?".into(),
        },
        DiffNote {
            scope: DiffScope::Branch,
            rev: None,
            path: "src/c.rs".into(),
            side: DiffSide::Old,
            start: 1,
            end: 2,
            text: "On the branch.".into(),
        },
    ];
    assert_eq!(parse_notes(&format_notes(&notes)).as_ref(), Some(&notes));
}

#[test]
fn a_resize_outside_the_terminal_limits_is_refused() {
    let resize = |cols, rows| {
        Op::Thread(ThreadOp::Resize {
            id: ThreadId("thr_1".into()),
            cols,
            rows,
            terminal: None,
        })
    };
    let refused = Err("terminal size must be 2 to 1000 columns and 1 to 500 rows");
    for (cols, rows) in [
        (0, 24),
        (1, 24),
        (80, 0),
        (1001, 24),
        (80, 501),
        (65535, 65535),
    ] {
        assert_eq!(crate::check(&resize(cols, rows)), refused, "{cols}x{rows}");
    }
    for (cols, rows) in [(2, 1), (80, 24), (1000, 500)] {
        assert_eq!(crate::check(&resize(cols, rows)), Ok(()), "{cols}x{rows}");
    }
}

#[test]
fn search_refusals_name_their_limits() {
    use crate::{SEARCH_LIMIT, SEARCH_TEXT_MAX};
    let search = |text: String, limit| {
        crate::check(&Op::Thread(ThreadOp::Search {
            text,
            limit,
            archived: false,
        }))
    };
    assert_eq!(
        search("x".repeat(SEARCH_TEXT_MAX + 1), None).unwrap_err(),
        format!("search text is longer than {SEARCH_TEXT_MAX} characters")
    );
    assert!(search("x".repeat(SEARCH_TEXT_MAX), None).is_ok());
    assert_eq!(
        search("x".into(), Some(SEARCH_LIMIT + 1)).unwrap_err(),
        format!("--limit must be from 1 to {SEARCH_LIMIT}")
    );
    assert!(search("x".into(), Some(SEARCH_LIMIT)).is_ok());
}
