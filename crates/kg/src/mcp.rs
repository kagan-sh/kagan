use std::io::{self, BufRead, Write};

use kg_domain::{Op, OpError, OpResult, ThreadId, ThreadOp};
use serde::Deserialize;
use serde_json::{Value, json};

const MODERN: &[&str] = &["2026-07-28"];
const LEGACY: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];
const VERSION_KEY: &str = "io.modelcontextprotocol/protocolVersion";

#[derive(Deserialize)]
struct Request {
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

struct Error {
    code: i64,
    message: String,
    data: Option<Value>,
}

impl Error {
    fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }
}

// Speaks both MCP eras: the `initialize` handshake and per-request `_meta` (2026-07-28).
pub(crate) fn run(
    thread: &ThreadId,
    input: impl BufRead,
    mut output: impl Write,
    mut call: impl FnMut(Op) -> Result<OpResult, OpError>,
) -> io::Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let (id, reply) = match serde_json::from_str::<Request>(&line) {
            Ok(request) => {
                let Some(id) = request.id else { continue };
                (
                    id,
                    handle(thread, &request.method, &request.params, &mut call),
                )
            }
            Err(error) => (Value::Null, Err(Error::new(-32700, error.to_string()))),
        };
        let message = match reply {
            Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
            Err(error) => {
                let mut body = json!({ "code": error.code, "message": error.message });
                if let Some(data) = error.data {
                    body["data"] = data;
                }
                json!({ "jsonrpc": "2.0", "id": id, "error": body })
            }
        };
        serde_json::to_writer(&mut output, &message)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
    Ok(())
}

fn handle(
    thread: &ThreadId,
    method: &str,
    params: &Value,
    call: &mut impl FnMut(Op) -> Result<OpResult, OpError>,
) -> Result<Value, Error> {
    if let Some(requested) = params["_meta"][VERSION_KEY].as_str()
        && !MODERN.contains(&requested)
    {
        return Err(Error {
            code: -32022,
            message: "Unsupported protocol version".into(),
            data: Some(json!({ "supported": MODERN, "requested": requested })),
        });
    }
    let server = json!({ "name": "kagan", "version": env!("CARGO_PKG_VERSION") });
    match method {
        "initialize" => {
            let requested = params["protocolVersion"].as_str().unwrap_or_default();
            let version = LEGACY
                .iter()
                .find(|&&known| known == requested)
                .unwrap_or(&LEGACY[0]);
            let (instructions, _) = tools(thread, call)?;
            Ok(with_instructions(
                json!({
                    "protocolVersion": version,
                    "capabilities": { "tools": {} },
                    "serverInfo": server,
                }),
                instructions,
            ))
        }
        "server/discover" => {
            let (instructions, _) = tools(thread, call)?;
            Ok(with_instructions(
                json!({
                    "resultType": "complete",
                    "supportedVersions": MODERN.iter().chain(LEGACY).collect::<Vec<_>>(),
                    "capabilities": { "tools": {} },
                    "_meta": { "io.modelcontextprotocol/serverInfo": server },
                    "ttlMs": 0,
                    "cacheScope": "private",
                }),
                instructions,
            ))
        }
        "ping" => Ok(json!({})),
        "tools/list" => {
            let (_, tools) = tools(thread, call)?;
            Ok(json!({
                "resultType": "complete",
                "tools": tools,
                "ttlMs": 0,
                "cacheScope": "private",
            }))
        }
        "tools/call" => {
            let Some(name) = params["name"].as_str() else {
                return Err(Error::new(-32602, "tools/call needs a name"));
            };
            let op = Op::Thread(ThreadOp::CallTool {
                id: thread.clone(),
                name: name.to_owned(),
                input: match &params["arguments"] {
                    Value::Null => json!({}),
                    arguments => arguments.clone(),
                },
            });
            let (text, is_error) = match call(op) {
                Ok(OpResult::ToolReply { text, is_error }) => (text, is_error),
                Ok(other) => return Err(unexpected(&other)),
                Err(error) => (error.to_string(), true),
            };
            Ok(json!({
                "resultType": "complete",
                "content": [{ "type": "text", "text": text }],
                "isError": is_error,
            }))
        }
        _ => Err(Error::new(-32601, format!("unknown method {method}"))),
    }
}

fn tools(
    thread: &ThreadId,
    call: &mut impl FnMut(Op) -> Result<OpResult, OpError>,
) -> Result<(Option<String>, Value), Error> {
    match call(Op::Thread(ThreadOp::Tools { id: thread.clone() })) {
        Ok(OpResult::Tools {
            instructions,
            tools,
        }) => Ok((instructions, json!(tools))),
        Ok(other) => Err(unexpected(&other)),
        Err(error) => Err(Error::new(-32603, error.to_string())),
    }
}

fn with_instructions(mut result: Value, instructions: Option<String>) -> Value {
    if let Some(instructions) = instructions {
        result["instructions"] = instructions.into();
    }
    result
}

fn unexpected(result: &OpResult) -> Error {
    Error::new(-32603, format!("unexpected reply from kagan: {result:?}"))
}
