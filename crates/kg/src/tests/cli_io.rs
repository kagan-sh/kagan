use kg_domain::{
    ConfigOp, EnvOp, IndexOp, Op, OpError, PluginId, PluginOp, ProjectId, TEXT_MAX, ThreadId,
    ThreadOp,
};
use serde_json::json;

use clap::Parser;

use super::cli::parse;
use crate::Cli;

#[test]
fn a_setting_left_off_the_command_line_is_read_as_text_and_never_parsed() {
    use crate::cli::take_value_with;
    let set = |value| {
        Op::Plugin(PluginOp::Config {
            id: PluginId("p".into()),
            op: ConfigOp::Set {
                key: "token".into(),
                value,
            },
            thread_id: None,
        })
    };
    let typed = parse(&["plugin", "config", "p", "set", "token"]);
    assert_eq!(typed, set(None));
    let read = take_value_with(typed.clone(), |key| {
        assert_eq!(key, "token");
        Ok("12345\r\n".into())
    });
    assert_eq!(read.unwrap(), set(Some(json!("12345"))));
    let empty = take_value_with(typed, |_| Ok("\n".into())).unwrap_err();
    assert!(
        empty.to_string().contains("type it at the prompt"),
        "{empty}"
    );
    let given = set(Some(json!(false)));
    assert_eq!(
        take_value_with(given.clone(), |_| panic!("not read")).unwrap(),
        given
    );
    assert_eq!(
        parse(&["plugin", "config", "p", "clear", "token"]),
        Op::Plugin(PluginOp::Config {
            id: PluginId("p".into()),
            op: ConfigOp::Clear {
                key: "token".into()
            },
            thread_id: None,
        })
    );
}

#[test]
fn paste_reads_a_file_or_stdin_and_sends_the_text() {
    use crate::cli::take_file_text;
    let id = ThreadId("thr_1".into());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    let body = "say \"hi\" and `date`";
    std::fs::write(&path, body).unwrap();
    let from_file = take_file_text(
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: None,
            file: Some(path.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap();
    let sent = Op::Thread(ThreadOp::Paste {
        id: id.clone(),
        text: Some(body.into()),
        file: None,
        terminal: None,
    });
    assert_eq!(from_file, sent);
    assert_eq!(
        take_file_text(
            Op::Thread(ThreadOp::Paste {
                id: id.clone(),
                text: None,
                file: Some("-".into()),
                terminal: None,
            }),
            std::io::Cursor::new(body),
        )
        .unwrap(),
        sent
    );
    let unchanged = Op::Thread(ThreadOp::Paste {
        id: id.clone(),
        text: Some("hello".into()),
        file: None,
        terminal: None,
    });
    assert_eq!(
        take_file_text(unchanged.clone(), std::io::empty()).unwrap(),
        unchanged
    );
    assert_eq!(
        parse(&["thread", "paste", "thr_1", "hello"]),
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: Some("hello".into()),
            file: None,
            terminal: None,
        })
    );
    assert_eq!(
        parse(&["thread", "paste", "thr_1", "--file", "-"]),
        Op::Thread(ThreadOp::Paste {
            id: id.clone(),
            text: None,
            file: Some("-".into()),
            terminal: None,
        })
    );
    assert!(
        Cli::try_parse_from(["kg", "thread", "paste", "thr_1", "hello", "--file", "a"]).is_err()
    );
    let big = dir.path().join("big");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(1024 * 1024 + 8)
        .unwrap();
    let err = take_file_text(
        Op::Thread(ThreadOp::Paste {
            id,
            text: None,
            file: Some(big.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap_err();
    let message = err.to_string();
    assert!(matches!(err, OpError::Invalid(_)));
    assert!(message.contains("1048584"), "{message}");
    assert!(message.contains("1 MiB"), "{message}");
}

#[test]
fn a_paste_file_past_the_cap_reports_its_real_size() {
    use crate::cli::take_file_text;
    let dir = tempfile::tempdir().unwrap();
    let big = dir.path().join("big");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(TEXT_MAX as u64 + 8193)
        .unwrap();
    let err = take_file_text(
        Op::Thread(ThreadOp::Paste {
            id: ThreadId("thr_1".into()),
            text: None,
            file: Some(big.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("1056769 bytes"), "{err}");
}

#[test]
fn stdin_at_the_cap_is_accepted_and_one_byte_over_is_refused() {
    use crate::cli::take_file_text;
    let paste = || {
        Op::Thread(ThreadOp::Paste {
            id: ThreadId("thr_1".into()),
            text: None,
            file: Some("-".into()),
            terminal: None,
        })
    };
    assert!(take_file_text(paste(), std::io::Cursor::new(vec![b'a'; TEXT_MAX])).is_ok());
    let err = take_file_text(paste(), std::io::Cursor::new(vec![b'a'; TEXT_MAX + 1])).unwrap_err();
    assert!(matches!(err, OpError::Invalid(_)));
}

#[test]
fn paste_rejects_invalid_utf8_from_a_file_or_stdin() {
    use crate::cli::take_file_text;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad");
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    let pasted = Op::Thread(ThreadOp::Paste {
        id: ThreadId("thr_1".into()),
        text: None,
        file: Some(path.display().to_string()),
        terminal: None,
    });
    let from_file = take_file_text(pasted, std::io::empty()).unwrap_err();
    assert!(from_file.to_string().contains("UTF-8"), "{from_file}");
    let from_stdin = take_file_text(
        Op::Thread(ThreadOp::Paste {
            id: ThreadId("thr_1".into()),
            text: None,
            file: Some("-".into()),
            terminal: None,
        }),
        std::io::Cursor::new([0xff, 0xfe]),
    )
    .unwrap_err();
    assert!(from_stdin.to_string().contains("UTF-8"), "{from_stdin}");
}

#[cfg(unix)]
#[test]
fn paste_rejects_a_fifo_past_the_cap() {
    use std::io::Write;

    use crate::cli::take_file_text;
    let dir = tempfile::tempdir().unwrap();
    let fifo = dir.path().join("paste.fifo");
    assert!(
        std::process::Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .unwrap()
            .success()
    );
    let writing = fifo.clone();
    let writer = std::thread::spawn(move || {
        let Ok(mut file) = std::fs::OpenOptions::new().write(true).open(&writing) else {
            return;
        };
        let chunk = vec![b'a'; 64 * 1024];
        let mut sent = 0usize;
        while sent < 2 * 1024 * 1024 {
            if file.write_all(&chunk).is_err() {
                break;
            }
            sent += chunk.len();
        }
    });
    let streamed = take_file_text(
        Op::Thread(ThreadOp::Paste {
            id: ThreadId("thr_1".into()),
            text: None,
            file: Some(fifo.display().to_string()),
            terminal: None,
        }),
        std::io::empty(),
    )
    .unwrap_err();
    let _ = writer.join();
    let streamed = streamed.to_string();
    assert!(streamed.contains("1 MiB"), "{streamed}");
    assert!(!streamed.contains("2097152"), "{streamed}");
}

#[test]
fn browser_fill_reads_a_file_or_stdin_and_sends_the_text() {
    use crate::cli::take_file_text;
    let thread_id = ThreadId("thr_1".into());
    let fill = |ref_id: u32, text: Option<&str>, file: Option<std::path::PathBuf>| {
        Op::Browser(kg_domain::BrowserOp::Fill {
            ref_id,
            text: text.map(str::to_owned),
            file,
            thread_id: thread_id.clone(),
        })
    };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("note.txt");
    let body = "say \"hi\" and `date`";
    std::fs::write(&path, body).unwrap();
    let from_file = take_file_text(fill(3, None, Some(path.clone())), std::io::empty()).unwrap();
    let sent = fill(3, Some(body), None);
    assert_eq!(from_file, sent);
    assert_eq!(
        take_file_text(
            fill(3, None, Some(std::path::PathBuf::from("-"))),
            std::io::Cursor::new(body),
        )
        .unwrap(),
        sent
    );
    let plain = fill(2, Some("ada@example.com"), None);
    assert_eq!(
        take_file_text(plain.clone(), std::io::empty()).unwrap(),
        plain
    );
    assert!(Cli::try_parse_from(["kg", "browser", "fill", "3", "ada", "--file", "a"]).is_err());
    let big = dir.path().join("big");
    std::fs::File::create(&big)
        .unwrap()
        .set_len(1024 * 1024 + 8)
        .unwrap();
    let err = take_file_text(fill(3, None, Some(big)), std::io::empty()).unwrap_err();
    let message = err.to_string();
    assert!(matches!(err, OpError::Invalid(_)));
    assert!(message.contains("text is 1048584 bytes"), "{message}");
    assert!(message.contains("1 MiB"), "{message}");
    let bad = dir.path().join("bad");
    std::fs::write(&bad, [0xff, 0xfe]).unwrap();
    let err = take_file_text(fill(3, None, Some(bad)), std::io::empty()).unwrap_err();
    assert!(err.to_string().contains("UTF-8"), "{err}");
    let missing = dir.path().join("missing");
    let err = take_file_text(fill(3, None, Some(missing)), std::io::empty()).unwrap_err();
    assert!(err.to_string().contains("could not read"), "{err}");
}

#[test]
fn install_sends_a_bare_name_to_the_indexes_and_a_folder_as_its_path() {
    use crate::cli::paths;
    let source = |args: &[&str]| match paths(parse(args)).unwrap() {
        Op::Plugin(PluginOp::Install { source, .. }) => source,
        Op::Plugin(PluginOp::Index {
            op: IndexOp::Add { location },
        }) => location,
        Op::Plugin(PluginOp::Search { text }) => text,
        op => panic!("{op:?}"),
    };
    assert_eq!(
        source(&["plugin", "install", "no-such-folder-here"]),
        "no-such-folder-here"
    );
    let folder = std::env::current_dir().unwrap().join(".");
    assert_eq!(
        source(&["plugin", "install", "."]),
        folder.display().to_string()
    );
    assert_eq!(
        source(&["plugin", "index", "add", "."]),
        folder.display().to_string()
    );
    assert_eq!(
        source(&["plugin", "search"]),
        String::new(),
        "search with no text lists every plugin"
    );
}

#[test]
fn an_env_value_is_read_from_stdin_and_never_taken_from_the_command_line() {
    use crate::cli::take_value_with;
    let typed = parse(&["env", "set", "API_KEY", "--project", "prj_1"]);
    let project = Some(ProjectId("prj_1".into()));
    assert_eq!(
        typed,
        Op::Env(EnvOp::Set {
            name: "API_KEY".into(),
            project_id: project.clone(),
            value: None,
        })
    );
    let read = take_value_with(typed, |name| {
        assert_eq!(name, "API_KEY");
        Ok("sk-1\n".into())
    });
    assert_eq!(
        read.unwrap(),
        Op::Env(EnvOp::Set {
            name: "API_KEY".into(),
            project_id: project,
            value: Some("sk-1".into()),
        })
    );
    let empty = take_value_with(parse(&["env", "set", "API_KEY"]), |_| Ok("\n".into()));
    assert!(empty.is_err());
    assert!(Cli::try_parse_from(["kg", "env", "set", "API_KEY", "sk-1"]).is_err());
    assert_eq!(
        parse(&["env", "list"]),
        Op::Env(EnvOp::List { project_id: None })
    );
    assert_eq!(
        parse(&["env", "remove", "API_KEY"]),
        Op::Env(EnvOp::Remove {
            name: "API_KEY".into(),
            project_id: None,
        })
    );
}
