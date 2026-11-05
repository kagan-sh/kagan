#![expect(
    clippy::print_stdout,
    reason = "the CLI writes command output and results to stdout"
)]

mod app;
mod cli;
#[cfg(target_os = "macos")]
mod icon;
mod mcp;
#[cfg(windows)]
mod pipe;
mod socket;
#[cfg(test)]
mod tests;

use std::{ffi::OsString, io, process::ExitCode};

use clap::error::{ContextKind, ContextValue, ErrorKind};
use clap::{Parser, Subcommand};
use kg_domain::{Op, OpError, PluginOp, ProjectId, ThreadId, invalid_json};

use crate::socket::Client;

#[derive(Parser)]
#[command(name = "kg", version, about = "kagan")]
struct Cli {
    /// Print compact JSON; failures print the error envelope
    #[arg(long, global = true)]
    json: bool,
    #[arg(long, hide = true)]
    headless: bool,
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Serve a thread's agent tools to its agent over stdio MCP
    Mcp {
        #[arg(long)]
        thread: ThreadId,
    },
    #[command(flatten)]
    Op(Box<Op>),
    /// A command a plugin declares under `cli`
    #[command(external_subcommand)]
    Plugin(Vec<OsString>),
}

/// The built-in command clap would suggest for a mistyped first word.
fn suggestion(arguments: &[OsString]) -> Option<String> {
    let error = Op::augment_subcommands(clap::Command::new("kg"))
        .try_get_matches_from(
            std::iter::once(OsString::from("kg")).chain(arguments.iter().cloned()),
        )
        .err()?;
    match error.get(ContextKind::SuggestedSubcommand)? {
        ContextValue::Strings(names) => names.first().cloned(),
        _ => None,
    }
}

fn plugin_cli(arguments: Vec<OsString>) -> Result<Op, OpError> {
    let mut words = arguments.into_iter().map(|word| {
        word.into_string()
            .map_err(|_| OpError::Invalid("plugin command arguments need UTF-8".into()))
    });
    let name = words.next().transpose()?.unwrap_or_default();
    let command = words
        .next()
        .transpose()?
        .ok_or_else(|| OpError::Invalid(format!("kg {name} needs a command")))?;
    let id = |key| std::env::var(key).ok().filter(|value| !value.is_empty());
    Ok(Op::Plugin(PluginOp::Cli {
        name,
        command,
        argv: words.collect::<Result<_, _>>()?,
        cwd: std::env::current_dir()?,
        thread_id: id("KG_THREAD_ID").map(ThreadId),
        project_id: id("KG_PROJECT_ID").map(ProjectId),
    }))
}

fn id_hint(error: &clap::Error) -> Option<String> {
    if error.kind() != ErrorKind::MissingRequiredArgument {
        return None;
    }
    let ContextValue::Strings(missing) = error.get(ContextKind::InvalidArg)? else {
        return None;
    };
    let usage = match error.get(ContextKind::Usage) {
        Some(ContextValue::StyledStr(text)) => text.to_string(),
        Some(ContextValue::String(text)) => text.clone(),
        _ => String::new(),
    };
    let mut variable = None;
    for arg in missing {
        let next = id_variable(arg, &usage)?;
        if variable.is_some() && variable != Some(next) {
            return None;
        }
        variable = Some(next);
    }
    variable.map(|name| format!("set {name}, or pass the id"))
}

fn id_variable(arg: &str, usage: &str) -> Option<&'static str> {
    let name = arg.to_ascii_lowercase();
    if name.contains("thread") {
        return Some("KG_THREAD_ID");
    }
    if name.contains("project") {
        return Some("KG_PROJECT_ID");
    }
    if name.contains("environment") {
        return Some("KG_ENVIRONMENT_ID");
    }
    if name != "<id>" {
        return None;
    }
    for word in usage.split_whitespace() {
        match word.trim_matches(|c: char| !c.is_ascii_alphanumeric()) {
            "thread" => return Some("KG_THREAD_ID"),
            "project" => return Some("KG_PROJECT_ID"),
            "environment" => return Some("KG_ENVIRONMENT_ID"),
            _ => {}
        }
    }
    None
}

fn json_error(error: &clap::Error) -> String {
    invalid_json(&error.to_string(), id_hint(error).as_deref().unwrap_or(""))
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            error.exit()
        }
        Err(error) => {
            if std::env::args_os().any(|argument| argument == "--json") {
                println!("{}", json_error(&error));
            } else {
                eprint!("{error}");
                if error.kind() == ErrorKind::MissingRequiredArgument
                    && std::env::args().skip(1).collect::<Vec<_>>() == ["plugin", "new"]
                {
                    eprintln!("Try: kg plugin new hello");
                }
            }
            return ExitCode::FAILURE;
        }
    };
    if cli.headless {
        return match app::run() {
            Ok(()) => ExitCode::SUCCESS,
            Err(error) => {
                eprintln!("kg: {error:#}");
                ExitCode::FAILURE
            }
        };
    }
    let reply = match cli.command.unwrap_or(Command::Op(Box::new(Op::Open))) {
        Command::Mcp { thread } => socket::location()
            .and_then(|location| socket::before_connect(&location).map(|()| location))
            .and_then(|location| socket::name(&location))
            .and_then(Client::connect)
            .and_then(|mut client| {
                mcp::run(&thread, io::stdin().lock(), io::stdout().lock(), |op| {
                    client.call(&op)
                })
            })
            .map_err(OpError::from),
        Command::Op(op) => cli::run(*op, cli.json),
        Command::Plugin(arguments) => {
            let typed = arguments.clone();
            plugin_cli(arguments)
                .and_then(|op| cli::run(op, cli.json))
                .map_err(|error| match error {
                    OpError::NotFound(message) => OpError::NotFound(match suggestion(&typed) {
                        Some(name) => format!("{message}; did you mean {name}?"),
                        None => message,
                    }),
                    other => other,
                })
        }
    };
    match reply {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if cli.json {
                println!("{}", serde_json::json!(error));
            } else {
                eprintln!("kg: {error}");
            }
            ExitCode::FAILURE
        }
    }
}
