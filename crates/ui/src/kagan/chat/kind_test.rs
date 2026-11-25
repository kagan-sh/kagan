use super::*;
use kg_domain::{Timestamp, ToolCall, ToolKind, ToolStatus};

fn tool(name: Option<&str>, kind: ToolKind, title: &str, input: serde_json::Value) -> ToolRow {
    ToolRow {
        call: ToolCall {
            id: "t1".into(),
            name: name.map(str::to_owned),
            kind: Some(kind),
            title: Some(title.into()),
            status: Some(ToolStatus::Completed),
            input: Some(input),
            output: None,
            diff: Vec::new(),
            locations: Vec::new(),
            approval: None,
        },
        counts: (0, 0),
        from: Timestamp(0),
        to: Timestamp(1000),
    }
}

#[test]
fn an_mcp_name_reads_as_its_server_and_tool() {
    assert_eq!(
        of(&tool(
            Some("mcp__github__create_issue"),
            ToolKind::Other,
            "Create issue",
            serde_json::json!({}),
        )),
        WorkKind::Mcp {
            server: "github".into(),
            tool: "create_issue".into(),
        }
    );
}

#[test]
fn kagans_own_server_is_recognised_by_its_name() {
    assert_eq!(
        of(&tool(
            Some("kagan_open_pull_request"),
            ToolKind::Other,
            "Open PR",
            serde_json::json!({}),
        )),
        WorkKind::Mcp {
            server: "kagan".into(),
            tool: "open_pull_request".into(),
        }
    );
}

#[test]
fn a_skill_load_is_named_by_its_input() {
    let kind = of(&tool(
        Some("skill_load"),
        ToolKind::Other,
        "Load a skill",
        serde_json::json!({"name": "rust-review"}),
    ));
    assert_eq!(
        kind,
        WorkKind::Skill {
            name: "rust-review".into()
        }
    );
    let title_only = of(&tool(
        None,
        ToolKind::Other,
        "load skill playwright-debugging",
        serde_json::json!({"name": "playwright-debugging"}),
    ));
    assert_eq!(
        title_only,
        WorkKind::Skill {
            name: "playwright-debugging".into()
        }
    );
}

#[test]
fn an_unknown_agent_tool_keeps_its_kind_and_title() {
    let kind = of(&tool(
        Some("some_agent_tool"),
        ToolKind::Read,
        "Read file",
        serde_json::json!({"path": "a.ts"}),
    ));
    assert_eq!(kind, WorkKind::Read);
    assert_eq!(
        title(
            &kind,
            &tool(
                Some("some_agent_tool"),
                ToolKind::Read,
                "Read file",
                serde_json::json!({"path": "a.ts"}),
            )
        ),
        Some("a.ts".into())
    );
}

#[test]
fn a_row_shows_a_span_only_once_it_has_run() {
    let mut row = tool(None, ToolKind::Execute, "cargo t", serde_json::json!({}));
    row.call.status = Some(ToolStatus::Pending);
    assert_eq!(meta(&WorkKind::Execute, &row), None);
    row.call.status = Some(ToolStatus::InProgress);
    assert_eq!(meta(&WorkKind::Execute, &row).as_deref(), Some("1 s"));
}

#[test]
fn a_title_is_cut_at_a_word() {
    assert_eq!(cut("one two three", 60), "one two three");
    assert_eq!(cut("one two three four", 9), "one two…");
}

#[test]
fn a_span_reads_in_seconds_then_minutes() {
    assert_eq!(span_text(Duration::from_millis(300)), "1 s");
    assert_eq!(span_text(Duration::from_secs(18)), "18 s");
    assert_eq!(span_text(Duration::from_secs(72)), "1 min 12 s");
}
