use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::future::Future;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

use async_channel::{Receiver, Sender};
use futures::FutureExt as _;
use futures::io::{AsyncRead, AsyncReadExt as _, AsyncWrite, AsyncWriteExt as _};
use gpui_kit::{Entity, Task};
use gpui_shell::{HostArguments, HostError, HostModule};
use kg_process::{Process, find_program, pathext};
use serde::Deserialize;
use serde_json::json;

use crate::area::{from_host, to_host, with_app};
use crate::manifest::{Env, Grant};

#[derive(Default)]
pub(crate) struct Processes {
    next: u64,
    running: BTreeMap<String, Running>,
}

struct Running {
    kill: Sender<()>,
    finished: Receiver<()>,
    task: Task<()>,
}

impl Processes {
    // Each receiver closes once its worker has killed the tree and dropped it.
    pub(crate) fn close(&mut self) -> Vec<Receiver<()>> {
        for running in self.running.values() {
            running.kill.close();
        }
        // Keep workers alive to reap their children after they kill the tree.
        std::mem::take(&mut self.running)
            .into_values()
            .map(|running| {
                running.task.detach();
                running.finished
            })
            .collect()
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RunOptions {
    cwd: Option<PathBuf>,
    stdin: Option<String>,
    timeout_ms: Option<f64>,
}

fn parse<T: serde::de::DeserializeOwned + Default>(args: &HostArguments) -> Result<T, HostError> {
    args.get(2).map_or_else(
        || Ok(T::default()),
        |value| serde_json::from_value(from_host(value)).map_err(error),
    )
}

fn error(error: impl std::fmt::Display) -> HostError {
    HostError::new(error.to_string())
}

fn env_key(key: &OsStr) -> String {
    let mut key = key.to_string_lossy().into_owned();
    if cfg!(windows) {
        key.make_ascii_uppercase();
    }
    key
}

fn env_allowed(key: &OsStr) -> bool {
    let key = env_key(key);
    matches!(
        key.as_str(),
        "HOME"
            | "USERPROFILE"
            | "APPDATA"
            | "LOCALAPPDATA"
            | "SSH_AUTH_SOCK"
            | "LANG"
            | "TERM"
            | "PATH"
            | "KG_DATA_DIR"
            | "KG_SOCKET"
    ) || key.starts_with("XDG_")
        || key.starts_with("GH_")
}

fn filtered_env(
    vars: impl Iterator<Item = (OsString, OsString)>,
    inherit: bool,
) -> Vec<(OsString, OsString)> {
    vars.filter(|(key, _)| inherit || env_allowed(key))
        .collect()
}

fn environment(inherit: bool) -> Vec<(OsString, OsString)> {
    filtered_env(std::env::vars_os(), inherit)
}

fn command(
    program: &Path,
    args: &[String],
    cwd: &Path,
    inherit: bool,
) -> Result<Command, HostError> {
    let env = environment(inherit);
    let path = env
        .iter()
        .find(|(key, _)| env_key(key) == "PATH")
        .map(|(_, value)| value.as_os_str());
    let program = find_program(program, path, pathext().as_deref())
        .ok_or_else(|| HostError::new(format!("program {} not found", program.display())))?;
    let mut command = Command::new(program);
    command.args(args).env_clear().envs(env).current_dir(cwd);
    Ok(command)
}

async fn pipes(
    mut stdin: impl AsyncWrite + Unpin,
    mut stdout: impl AsyncRead + Unpin,
    mut stderr: impl AsyncRead + Unpin,
    status: impl Future<Output = Result<ExitStatus, HostError>>,
    input: Option<String>,
) -> Result<serde_json::Value, HostError> {
    let input = async {
        if let Some(input) = input {
            stdin.write_all(input.as_bytes()).await.map_err(error)?;
        }
        drop(stdin);
        Ok::<_, HostError>(())
    };
    let out = async {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).await.map_err(error)?;
        Ok::<_, HostError>(bytes)
    };
    let err = async {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.map_err(error)?;
        Ok::<_, HostError>(bytes)
    };
    let ((), stdout, stderr, status) = futures::try_join!(input, out, err, status)?;
    Ok(json!({
        "exitCode": status.code(),
        "stdout": String::from_utf8_lossy(&stdout),
        "stderr": String::from_utf8_lossy(&stderr)
    }))
}

fn run(module: HostModule, owner: &Entity<Processes>, grant: &Grant, root: &Path) -> HostModule {
    let run_owner = owner.clone();
    let run = grant.powers.run.clone();
    let run_root = root.to_owned();
    module.async_function("run", move |arguments| {
        let program = arguments.string(0)?;
        let entry = run
            .iter()
            .find(|entry| entry.program == program)
            .ok_or_else(|| HostError::new(format!("program {program} is not granted for run")))?;
        let inherit = matches!(entry.env, Some(Env::Inherit));
        let args: Vec<String> =
            serde_json::from_value(from_host(arguments.value(1)?)).map_err(error)?;
        let options: RunOptions = parse(arguments)?;
        let program = PathBuf::from(program);
        let cwd = options.cwd.unwrap_or_else(|| run_root.clone());
        let timeout = options
            .timeout_ms
            .map_or_else(
                || Ok(Duration::from_mins(1)),
                |ms| Duration::try_from_secs_f64(ms / 1000.0),
            )
            .map_err(error)?;
        let (kill, killed) = async_channel::bounded(1);
        let (finish, exited) = async_channel::bounded::<()>(1);
        let task = with_app(|cx| {
            let executor = cx.background_executor().clone();
            let timer_executor = executor.clone();
            let task = executor.spawn(async move {
                let _finish = finish;
                if killed.is_closed() {
                    return Err(HostError::new("the plugin was unloaded"));
                }
                let mut process = Process::spawn(
                    command(&program, &args, &cwd, inherit)?,
                    Stdio::piped(),
                    Stdio::piped(),
                    Stdio::piped(),
                )
                .map_err(error)?;
                let child = process.child();
                let stdin = child
                    .stdin
                    .take()
                    .ok_or_else(|| HostError::new("missing stdin"))?;
                let stdout = child
                    .stdout
                    .take()
                    .ok_or_else(|| HostError::new("missing stdout"))?;
                let stderr = child
                    .stderr
                    .take()
                    .ok_or_else(|| HostError::new("missing stderr"))?;
                let status = async {
                    let status = process.child().status().await.map_err(error)?;
                    process.kill();
                    Ok(status)
                };
                let output = async {
                    Ok(to_host(
                        pipes(stdin, stdout, stderr, status, options.stdin).await?,
                    ))
                }
                .fuse();
                let timer = timer_executor.timer(timeout).fuse();
                let cancelled = killed.recv().fuse();
                futures::pin_mut!(output, timer, cancelled);
                futures::select_biased! {
                    result = output => result,
                    _ = cancelled => Err(HostError::new("the plugin was unloaded")),
                    () = timer => Err(HostError::new("process timed out")),
                }
            });
            let (done, reply) = async_channel::bounded(1);
            let id = run_owner.update(cx, |owner, _| {
                owner.next += 1;
                owner.next.to_string()
            });
            let finished = id.clone();
            let owner = run_owner.downgrade();
            let task = cx.spawn(async move |cx| {
                let _ = done.send(task.await).await;
                let _ = owner.update(cx, |owner, _| {
                    owner.running.remove(&finished);
                });
            });
            run_owner.update(cx, |owner, _| {
                owner.running.insert(
                    id,
                    Running {
                        kill,
                        finished: exited,
                        task,
                    },
                );
            });
            reply
        })?;
        Ok(async move { task.recv().await.map_err(error)? })
    })
}

pub(crate) const DECLARATIONS: &str = "export function run(program: string, args: string[], options?: { cwd?: string; stdin?: string; timeoutMs?: number }): Promise<{ exitCode: number | null; stdout: string; stderr: string }>;";

pub(crate) fn module(owner: &Entity<Processes>, grant: &Grant, root: &Path) -> HostModule {
    run(
        HostModule::new("kagan/process").declarations(DECLARATIONS),
        owner,
        grant,
        root,
    )
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_keeps_env_key_case_exact_except_on_windows() {
        let vars = [
            ("HOME", "home"),
            ("Home", "folded"),
            ("PATH", "path"),
            ("Path", "folded-path"),
            ("XDG_RUNTIME_DIR", "xdg"),
            ("xdg_runtime_dir", "folded-xdg"),
            ("KG_DATA_DIR", "data"),
            ("kg_data_dir", "folded-data"),
            ("GH_TOKEN", "gh"),
            ("gh_token", "folded-gh"),
            ("SECRET", "no"),
        ]
        .into_iter()
        .map(|(key, value)| {
            (
                std::ffi::OsString::from(key),
                std::ffi::OsString::from(value),
            )
        });
        let kept: Vec<_> = filtered_env(vars, false)
            .into_iter()
            .map(|(key, value)| {
                (
                    key.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
            .collect();
        #[cfg(windows)]
        let expected = [
            ("HOME", "home"),
            ("Home", "folded"),
            ("PATH", "path"),
            ("Path", "folded-path"),
            ("XDG_RUNTIME_DIR", "xdg"),
            ("xdg_runtime_dir", "folded-xdg"),
            ("KG_DATA_DIR", "data"),
            ("kg_data_dir", "folded-data"),
            ("GH_TOKEN", "gh"),
            ("gh_token", "folded-gh"),
        ];
        #[cfg(not(windows))]
        let expected = [
            ("HOME", "home"),
            ("PATH", "path"),
            ("XDG_RUNTIME_DIR", "xdg"),
            ("KG_DATA_DIR", "data"),
            ("GH_TOKEN", "gh"),
        ];
        assert_eq!(
            kept,
            expected.map(|(key, value)| (key.to_owned(), value.to_owned()))
        );
    }
}
