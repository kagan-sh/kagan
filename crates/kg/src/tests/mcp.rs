use kg_domain::{AgentTool, Op, OpError, OpResult, ThreadId, ThreadOp};
use serde_json::{Value, json};

use crate::mcp;

fn fake(op: Op) -> Result<OpResult, OpError> {
    match op {
        Op::Thread(ThreadOp::Tools { .. }) => Ok(OpResult::Tools {
            instructions: Some("Use editor_open to show a file.".into()),
            tools: vec![AgentTool {
                name: "editor_open".into(),
                description: "Open a file".into(),
                input_schema: json!({ "type": "object" }),
            }],
        }),
        Op::Thread(ThreadOp::CallTool { name, input, .. }) if name == "editor_open" => {
            Ok(OpResult::ToolReply {
                text: format!("opened {}", input["path"]),
                is_error: false,
            })
        }
        _ => Err(OpError::NotFound("no such tool".into())),
    }
}

fn mcp(requests: &[Value]) -> Vec<Value> {
    let input = requests
        .iter()
        .map(Value::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    let mut output = Vec::new();
    mcp::run(
        &ThreadId("thr_1".into()),
        input.as_bytes(),
        &mut output,
        fake,
    )
    .unwrap();
    String::from_utf8(output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn mcp_legacy_session() {
    let replies = mcp(&[
        json!({ "jsonrpc": "2.0", "id": 1, "method": "initialize",
                "params": { "protocolVersion": "2025-06-18", "capabilities": {} } }),
        json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }),
        json!({ "jsonrpc": "2.0", "id": 2, "method": "tools/list" }),
        json!({ "jsonrpc": "2.0", "id": 3, "method": "tools/call",
                "params": { "name": "editor_open", "arguments": { "path": "a.rs" } } }),
        json!({ "jsonrpc": "2.0", "id": 4, "method": "tools/call",
                "params": { "name": "missing" } }),
        json!({ "jsonrpc": "2.0", "id": 5, "method": "resources/list" }),
    ]);
    assert_eq!(replies.len(), 5);
    assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(
        replies[0]["result"]["instructions"],
        "Use editor_open to show a file."
    );
    assert_eq!(
        replies[1]["result"]["tools"],
        json!([{ "name": "editor_open", "description": "Open a file",
                 "inputSchema": { "type": "object" } }])
    );
    assert_eq!(
        replies[2]["result"]["content"],
        json!([{ "type": "text", "text": "opened \"a.rs\"" }])
    );
    assert_eq!(replies[2]["result"]["isError"], false);
    assert_eq!(replies[3]["result"]["isError"], true);
    assert_eq!(replies[3]["result"]["content"][0]["text"], "no such tool");
    assert_eq!(replies[4]["error"]["code"], -32601);
}

#[test]
fn mcp_modern_requests() {
    let meta = |version: &str| json!({ "io.modelcontextprotocol/protocolVersion": version });
    let replies = mcp(&[
        json!({ "jsonrpc": "2.0", "id": "d", "method": "server/discover",
                "params": { "_meta": meta("2026-07-28") } }),
        json!({ "jsonrpc": "2.0", "id": "old", "method": "tools/list",
                "params": { "_meta": meta("1900-01-01") } }),
        json!({ "jsonrpc": "2.0", "id": 7, "method": "initialize",
                "params": { "protocolVersion": "1900-01-01" } }),
    ]);
    let discover = &replies[0]["result"];
    assert_eq!(discover["resultType"], "complete");
    assert_eq!(discover["supportedVersions"][0], "2026-07-28");
    assert_eq!(discover["instructions"], "Use editor_open to show a file.");
    assert_eq!(discover["ttlMs"], 0);
    assert_eq!(replies[1]["id"], "old");
    assert_eq!(replies[1]["error"]["code"], -32022);
    assert_eq!(
        replies[1]["error"]["data"]["supported"],
        json!(["2026-07-28"])
    );
    assert_eq!(replies[2]["result"]["protocolVersion"], "2025-11-25");
}

#[test]
fn mcp_bad_json() {
    let mut output = Vec::new();
    mcp::run(
        &ThreadId("thr_1".into()),
        "not json\n".as_bytes(),
        &mut output,
        fake,
    )
    .unwrap();
    let reply: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(reply["id"], Value::Null);
    assert_eq!(reply["error"]["code"], -32700);
}
#[test]
fn mcp_passes_skill_load_through_the_thread_context() {
    let request = json!({ "jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": { "name": "skill_load", "arguments": { "name": "kg-cli" } } }).to_string();
    let mut output = Vec::new();
    mcp::run(
        &ThreadId("thr_1".into()),
        request.as_bytes(),
        &mut output,
        |op| {
            assert_eq!(
                op,
                Op::Thread(ThreadOp::CallTool {
                    id: ThreadId("thr_1".into()),
                    name: "skill_load".into(),
                    input: json!({"name": "kg-cli"})
                })
            );
            Ok(OpResult::ToolReply {
                text: "skill body and files".into(),
                is_error: false,
            })
        },
    )
    .unwrap();
    let response: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(
        response["result"]["content"][0]["text"],
        "skill body and files"
    );
}
