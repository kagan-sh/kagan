use std::collections::BTreeMap;
use std::hash::{Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::time::Duration;

use kg_domain::{BrowserSnapshot, ConfigOp, Op, OpError, OpResult, PluginOp, ThreadOp, UiSnapshot};

use crate::socket::Client;

pub(crate) fn run(op: Op, json: bool) -> Result<(), OpError> {
    let op = take_value(take_paste(op)?)?;
    let next = match &op {
        Op::Plugin(PluginOp::New { name, dir }) => {
            Some((true, dir.clone().unwrap_or_else(|| PathBuf::from(name))))
        }
        Op::Plugin(PluginOp::Check { path }) => Some((false, path.clone())),
        _ => None,
    };
    let notice = match &op {
        Op::Plugin(PluginOp::Install { source, .. }) => {
            Some(format!("Installing {}", kg_plugins::shown(source)))
        }
        Op::Plugin(PluginOp::Remove { id, .. }) => Some(format!("Removing {id}")),
        Op::Project(kg_domain::ProjectOp::Remove { id, .. }) => Some(format!("Removing {id}")),
        _ => None,
    };
    let op = paths(op)?;
    let mut client = Client::connect_or_start()?;
    match op {
        Op::Plugin(PluginOp::Dev { path, thread_id }) => {
            let result = client.call(&Op::Plugin(PluginOp::Dev {
                path,
                thread_id: thread_id.clone(),
            }))?;
            let OpResult::Plugins(mut plugins) = result else {
                return Err(OpError::Failed("plugin dev returned no plugin".into()));
            };
            let plugin = plugins
                .pop()
                .ok_or_else(|| OpError::Failed("plugin dev returned no plugin".into()))?;
            let mut before = snapshot(&plugin.root)?;
            loop {
                std::thread::sleep(Duration::from_millis(500));
                let after = snapshot(&plugin.root)?;
                if after != before {
                    before = after;
                    let result = client.call(&Op::Plugin(PluginOp::Reload {
                        id: plugin.id.clone(),
                        thread_id: thread_id.clone(),
                    }));
                    match result {
                        Ok(result) => print(&result, json)?,
                        Err(error) if json => println!("{}", serde_json::to_string(&error)?),
                        Err(error) => eprintln!("kg: {error}"),
                    }
                }
            }
        }
        Op::Plugin(PluginOp::Logs {
            id,
            follow: true,
            after,
        }) => {
            let mut after = after;
            loop {
                let result = client.call(&Op::Plugin(PluginOp::Logs {
                    id: id.clone(),
                    follow: false,
                    after,
                }))?;
                let OpResult::Logs {
                    text,
                    after: next,
                    reset,
                } = &result
                else {
                    return Err(OpError::Failed("plugin logs returned no logs".into()));
                };
                if *reset && !json {
                    eprintln!("plugin log rotated; restarting at 0");
                }
                if !text.is_empty() || *reset {
                    print(&result, json)?;
                }
                after = Some(*next);
                std::thread::sleep(Duration::from_millis(500));
            }
        }
        op @ Op::Thread(ThreadOp::Ask { .. } | ThreadOp::Wait { .. }) if !json => {
            match client.call(&op)? {
                OpResult::Text(text) => println!("{text}"),
                OpResult::Wait(outcome) => println!("{outcome}"),
                result => print(&result, false)?,
            }
            Ok(())
        }
        op => {
            if let Some(notice) = notice {
                eprintln!("{notice}. kagan may ask for your OK in its window.");
            }
            let result = client.call(&op)?;
            print_result(&result, next, json)
        }
    }
}

fn print_result(
    result: &OpResult,
    next: Option<(bool, PathBuf)>,
    json: bool,
) -> Result<(), OpError> {
    if !json && let Some((created, path)) = next {
        if created {
            println!("Plugin created.\nNext: kg plugin check {}", path.display());
        } else {
            if !matches!(result, OpResult::Done) {
                print(result, false)?;
            }
            let install = if path.is_relative() && !path.starts_with(".") {
                PathBuf::from(".").join(&path)
            } else {
                path
            };
            println!(
                "Plugin check passed.\nNext: kg plugin install {}",
                install.display()
            );
        }
        Ok(())
    } else {
        print(result, json)
    }
}

fn print(result: &OpResult, json: bool) -> Result<(), OpError> {
    if !json {
        match result {
            OpResult::Cli {
                exit_code,
                stdout,
                stderr,
            } => {
                use std::io::Write as _;
                std::io::stdout().lock().write_all(stdout.as_bytes())?;
                std::io::stderr().lock().write_all(stderr.as_bytes())?;
                exit_with(*exit_code);
                return Ok(());
            }
            OpResult::Text(text) | OpResult::Logs { text, .. } => {
                use std::io::Write as _;
                std::io::stdout().lock().write_all(text.as_bytes())?;
                return Ok(());
            }
            OpResult::Wait(outcome) => {
                println!("{outcome}");
                return Ok(());
            }
            OpResult::Shared { entry, install } => {
                println!(
                    "Add this entry to plugins in your index's .kagan/index.json:\n  {}\nOr send a teammate this line:\n  {install}",
                    serde_json::to_string(entry)?
                );
                return Ok(());
            }
            OpResult::Diagnostics { path } => {
                use std::io::Write as _;
                std::io::stdout()
                    .lock()
                    .write_all(format!("{}\n", path.display()).as_bytes())?;
                return Ok(());
            }
            OpResult::Capture { path, current } => {
                use std::io::Write as _;
                std::io::stdout().lock().write_all(path.as_bytes())?;
                if !current {
                    std::io::stderr().lock().write_all(
                        b"\nkg: the window had not drawn a new frame; the image may show an earlier state\n",
                    )?;
                }
                return Ok(());
            }
            OpResult::Ui(snapshot) => {
                use std::io::Write as _;
                std::io::stdout()
                    .lock()
                    .write_all(format_ui_snapshot(snapshot).as_bytes())?;
                return Ok(());
            }
            OpResult::BrowserSnapshot(snapshot) => {
                use std::io::Write as _;
                std::io::stdout()
                    .lock()
                    .write_all(format_browser_snapshot(snapshot).as_bytes())?;
                return Ok(());
            }
            _ => {}
        }
    }
    let text = if json {
        serde_json::to_string(result)
    } else {
        serde_json::to_string_pretty(result)
    }?;
    println!("{text}");
    if let OpResult::Cli { exit_code, .. } = result {
        exit_with(*exit_code);
    }
    Ok(())
}

const PASTE_CAP: u64 = 1024 * 1024;

pub(crate) fn take_paste(op: Op) -> Result<Op, OpError> {
    take_paste_from(op, std::io::stdin().lock())
}

pub(crate) fn take_paste_from(op: Op, stdin: impl std::io::Read) -> Result<Op, OpError> {
    let Op::Thread(ThreadOp::Paste {
        id,
        text,
        file,
        terminal,
    }) = op
    else {
        return Ok(op);
    };
    let Some(path) = file else {
        return Ok(Op::Thread(ThreadOp::Paste {
            id,
            text,
            file: None,
            terminal,
        }));
    };
    if text.is_some() {
        return Err(OpError::Invalid("pass the text or --file, not both".into()));
    }
    let bytes = if path == "-" {
        read_capped(stdin)?
    } else {
        read_paste_file(&path)?
    };
    let text =
        String::from_utf8(bytes).map_err(|_| OpError::Invalid("paste must be UTF-8".into()))?;
    Ok(Op::Thread(ThreadOp::Paste {
        id,
        text: Some(text),
        file: None,
        terminal,
    }))
}

/// A setting value left off the command line comes from a prompt that does not show it, or
/// from stdin, so a secret stays out of shell history.
fn take_value(op: Op) -> Result<Op, OpError> {
    use std::io::IsTerminal as _;
    let stdin = std::io::stdin();
    if stdin.is_terminal() {
        take_value_with(op, |key| {
            Ok(rpassword::prompt_password(format!("{key} (not shown): "))?)
        })
    } else {
        take_value_with(op, |_| {
            String::from_utf8(read_capped(stdin.lock())?)
                .map_err(|_| OpError::Invalid("the value must be UTF-8".into()))
        })
    }
}

pub(crate) fn take_value_with(
    op: Op,
    read: impl FnOnce(&str) -> Result<String, OpError>,
) -> Result<Op, OpError> {
    let Op::Plugin(PluginOp::Config {
        id,
        op: ConfigOp::Set { key, value: None },
        thread_id,
    }) = op
    else {
        return Ok(op);
    };
    let text = read(&key)?;
    let line = text.strip_suffix('\n').unwrap_or(&text);
    let line = line.strip_suffix('\r').unwrap_or(line);
    if line.is_empty() {
        return Err(OpError::Invalid(format!(
            "no value for {key}: type it at the prompt, or pipe it on stdin"
        )));
    }
    Ok(Op::Plugin(PluginOp::Config {
        id,
        op: ConfigOp::Set {
            key,
            value: Some(line.into()),
        },
        thread_id,
    }))
}

fn read_paste_file(path: &str) -> Result<Vec<u8>, OpError> {
    let meta = std::fs::metadata(path)
        .map_err(|error| OpError::Invalid(format!("could not read {path}: {error}")))?;
    // A fifo reports length 0, and a file can grow after this check.
    if meta.is_file() && meta.len() > PASTE_CAP {
        return Err(OpError::Invalid(format!(
            "paste is {} bytes; the cap is 1 MiB",
            meta.len()
        )));
    }
    let file = std::fs::File::open(path)
        .map_err(|error| OpError::Invalid(format!("could not read {path}: {error}")))?;
    match read_capped(file) {
        Ok(bytes) => Ok(bytes),
        Err(OpError::Failed(message)) => {
            Err(OpError::Failed(format!("could not read {path}: {message}")))
        }
        Err(error) => Err(error),
    }
}

fn read_capped(mut reader: impl std::io::Read) -> Result<Vec<u8>, OpError> {
    let mut bytes = Vec::new();
    let mut chunk = [0_u8; 8192];
    loop {
        let read = reader
            .read(&mut chunk)
            .map_err(|error| OpError::Failed(error.to_string()))?;
        if read == 0 {
            break;
        }
        let next = bytes.len().saturating_add(read);
        if u64::try_from(next).unwrap_or(u64::MAX) > PASTE_CAP {
            return Err(OpError::Invalid(format!(
                "paste is {next} bytes; the cap is 1 MiB"
            )));
        }
        bytes.extend_from_slice(&chunk[..read]);
    }
    Ok(bytes)
}

pub(crate) fn format_browser_snapshot(snapshot: &BrowserSnapshot) -> String {
    let mut out = String::new();
    let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{}\n", snapshot.url));
    if !snapshot.title.is_empty() {
        let _ = std::fmt::Write::write_fmt(&mut out, format_args!("{}\n", snapshot.title));
    }
    for control in &snapshot.controls {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!("[{}] {} {:?}", control.ref_id, control.role, control.name),
        );
        if let Some(value) = &control.value {
            let _ = std::fmt::Write::write_fmt(&mut out, format_args!(" value={value:?}"));
        }
        out.push('\n');
    }
    if !snapshot.text.is_empty() {
        out.push_str("text:\n");
        out.push_str(&snapshot.text);
        out.push('\n');
    }
    out
}

fn format_ui_snapshot(snapshot: &UiSnapshot) -> String {
    let mut out = String::new();
    for element in &snapshot.elements {
        let _ = std::fmt::Write::write_fmt(
            &mut out,
            format_args!(
                "{} role={} label={:?} value={:?} bounds=({:.1},{:.1},{:.1},{:.1}) focused={} disabled={} selected={} visible={}\n",
                element.id,
                element.role.as_deref().unwrap_or("-"),
                element.label,
                element.value,
                element.bounds.x,
                element.bounds.y,
                element.bounds.width,
                element.bounds.height,
                element
                    .focused
                    .map(|focused| focused.to_string())
                    .as_deref()
                    .unwrap_or("-"),
                element
                    .disabled
                    .map(|disabled| disabled.to_string())
                    .as_deref()
                    .unwrap_or("-"),
                element
                    .selected
                    .map(|selected| selected.to_string())
                    .as_deref()
                    .unwrap_or("-"),
                element.visible,
            ),
        );
    }
    out
}

fn exit_with(code: i32) {
    if code != 0 {
        std::process::exit(code);
    }
}

fn snapshot(root: &Path) -> Result<BTreeMap<PathBuf, u64>, OpError> {
    fn visit(folder: &Path, files: &mut BTreeMap<PathBuf, u64>) -> Result<(), OpError> {
        let entries = match std::fs::read_dir(folder) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let kind = entry.file_type()?;
            if kind.is_dir() {
                if !matches!(entry.file_name().to_str(), Some(".git" | "node_modules")) {
                    visit(&entry.path(), files)?;
                }
            } else if kind.is_file() {
                let mut hash = std::collections::hash_map::DefaultHasher::new();
                let bytes = match std::fs::read(entry.path()) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(error) => return Err(error.into()),
                };
                bytes.hash(&mut hash);
                files.insert(entry.path(), hash.finish());
            }
        }
        Ok(())
    }
    let mut files = BTreeMap::new();
    visit(root, &mut files)?;
    Ok(files)
}

pub(crate) fn paths(mut op: Op) -> Result<Op, OpError> {
    let cwd = std::env::current_dir()?;
    let absolute = |path: &mut PathBuf| {
        if path.is_relative() {
            *path = cwd.join(&*path);
        }
    };
    match &mut op {
        Op::Plugin(
            PluginOp::Check { path }
            | PluginOp::Share { path }
            | PluginOp::Dev { path, .. }
            | PluginOp::Types { path },
        )
        | Op::Skill(kg_domain::SkillOp::Add { path, .. })
        | Op::Project(kg_domain::ProjectOp::Add { path })
        | Op::Diagnostics(kg_domain::DiagnosticsOp::Export { path: Some(path) }) => {
            absolute(path);
        }
        Op::Plugin(PluginOp::New { name, dir }) => {
            let path = dir.get_or_insert_with(|| PathBuf::from(name.as_str()));
            absolute(path);
        }
        Op::Plugin(
            PluginOp::Install { source, .. }
            | PluginOp::Index {
                op:
                    kg_domain::IndexOp::Add { location: source }
                    | kg_domain::IndexOp::Remove { location: source },
            },
        ) if !source.starts_with("git:") && Path::new(source.as_str()).exists() => {
            let mut path = PathBuf::from(&*source);
            absolute(&mut path);
            *source = path
                .to_str()
                .ok_or_else(|| OpError::Invalid("plugin path needs UTF-8".into()))?
                .into();
        }
        _ => {}
    }
    Ok(op)
}
