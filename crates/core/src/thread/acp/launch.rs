use std::ffi::OsStr;
use std::path::Path;

use agent_client_protocol::v1::schema::{EnvVariable, McpServer, McpServerStdio};
use gpui_kit::{AppContext as _, AsyncApp, WeakEntity};
use kg_domain::{Conversation, OpError, ThreadId, acp_command};

use crate::Workspace;
use crate::host::database::Result;
use crate::thread::agent::{self, Start};
use crate::thread::resolve_acp;
use crate::thread::terminal::utf8_locale;
use crate::workspace::closed;

use super::client::{Launch, Session};

/// Builds the command that starts the agent's ACP server: the launch command's program and its
/// arguments, in the thread's folder, with kagan's variables. The session's own part is the
/// conversation it loads and the MCP server it is offered.
pub(super) async fn launch(
    this: &WeakEntity<Workspace>,
    record: &kg_domain::Thread,
    folder: &Path,
    start: Start,
    resume: Option<Conversation>,
    cx: &mut AsyncApp,
) -> Result<Launch> {
    let environment = crate::host::program::program_environment_of(this, cx);
    let home = crate::host::program::program_home_of(this, cx);
    let command = acp_command(&record.provider)
        .ok_or_else(|| OpError::Invalid(format!("{} has no ACP command", record.provider)))?;
    let program = resolve_acp(
        &record.provider,
        command,
        environment.as_ref(),
        home.as_deref(),
    )
    .map_err(|error| OpError::Invalid(error.to_string()))?;
    let integration = agent::integrate(this, record, folder, start, cx).await?;
    let (cli, socket) = this
        .update(cx, |workspace, _| {
            (workspace.cli.clone(), workspace.socket.clone())
        })
        .map_err(closed)?;
    let mut env = integration.env;
    if let Some(locale) = cx.background_spawn(async { utf8_locale() }).await {
        env.push(("LANG".into(), locale.into()));
    }
    Ok(Launch {
        program: program.path,
        args: program.args,
        cwd: folder.to_owned(),
        env,
        session: Session {
            resume,
            servers: mcp_servers(&record.id, &cli, socket.as_deref()),
        },
    })
}

/// The server every ACP session is offered: kagan's own, for this thread, the same one a terminal
/// thread's config names.
fn mcp_servers(id: &ThreadId, cli: &Path, socket: Option<&OsStr>) -> Vec<McpServer> {
    let mut server = McpServerStdio::new("kagan", cli).args(vec![
        "mcp".to_owned(),
        "--thread".to_owned(),
        id.0.clone(),
    ]);
    if let Some(socket) = socket {
        server = server.env(vec![EnvVariable::new(
            "KG_SOCKET",
            socket.to_string_lossy(),
        )]);
    }
    vec![McpServer::Stdio(server)]
}
