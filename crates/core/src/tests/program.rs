#[cfg(unix)]
use super::fake_program;
#[cfg(unix)]
use super::{Fixture, op};
#[cfg(unix)]
use gpui_kit::TestAppContext;
#[cfg(unix)]
use kg_domain::{Op, OpError, OpResult, ProgramOp, ProjectId, ProjectOp};

#[test]
fn a_program_splits_on_spaces_outside_quotes() {
    let Err(error) = crate::thread::resolve_program("kg-no-such-program trailing", None, None)
    else {
        panic!("resolved");
    };
    assert_eq!(
        error.to_string(),
        "program kg-no-such-program was not found"
    );
    let line = if cfg!(windows) {
        "cmd.exe /c echo"
    } else {
        "/bin/sh -c echo"
    };
    let resolved = crate::thread::resolve_program(line, None, None).unwrap();
    let expected = if cfg!(windows) {
        ["/c", "echo"]
    } else {
        ["-c", "echo"]
    };
    assert_eq!(
        resolved.args,
        expected
            .into_iter()
            .map(std::ffi::OsString::from)
            .collect::<Vec<_>>()
    );
}

#[test]
#[cfg(unix)]
fn quotes_keep_spaces_and_empty_words_and_backslashes_are_plain() {
    let args = |line: &str| {
        crate::thread::resolve_program(line, None, None)
            .unwrap()
            .args
            .into_iter()
            .map(|arg| arg.into_string().unwrap())
            .collect::<Vec<_>>()
    };
    assert_eq!(args("/bin/sh -c 'echo a b'"), ["-c", "echo a b"]);
    assert_eq!(args("/bin/sh \"\" x"), ["", "x"]);
    assert_eq!(args("/bin/sh -c a\"b c\"d"), ["-c", "ab cd"]);
    assert_eq!(args("/bin/sh -c 'it\"s'"), ["-c", "it\"s"]);
    assert_eq!(args(r"/bin/sh -c C:\tools\x"), ["-c", r"C:\tools\x"]);
}

#[test]
fn a_program_with_a_bad_quote_or_no_name_is_refused() {
    assert_eq!(
        crate::thread::resolve_program("'/bin/sh -c x", None, None)
            .err()
            .map(|error| error.to_string()),
        Some("program has an unfinished quote".to_owned())
    );
    assert_eq!(
        crate::thread::resolve_program("\"\" -c x", None, None)
            .err()
            .map(|error| error.to_string()),
        Some("program is empty".to_owned())
    );
}

#[cfg(unix)]
#[test]
fn the_program_list_finds_known_agents_and_defaults_to_the_first_installed() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "echo 'claude 9.9.9 (test)'");
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        Some(std::ffi::OsStr::new("/bin/sh")),
        None,
        &[],
        &[
            "codex --resume".into(),
            "claude".into(),
            "'unfinished".into(),
        ],
        None,
    );
    let claude = list
        .programs
        .iter()
        .find(|p| p.command == "claude")
        .unwrap();
    assert_eq!(claude.source, kg_domain::ProgramSource::Known);
    assert!(claude.installed());
    let codex = list.programs.iter().find(|p| p.command == "codex").unwrap();
    assert!(!codex.installed());
    assert!(
        list.programs
            .iter()
            .any(|p| p.source == kg_domain::ProgramSource::Shell && p.installed())
    );
    let recent = list
        .programs
        .iter()
        .find(|p| p.command == "codex --resume")
        .unwrap();
    assert_eq!(recent.source, kg_domain::ProgramSource::Recent);
    assert!(list.programs.iter().all(|p| p.command != "'unfinished"));
    assert_eq!(list.default.as_deref(), Some("claude"));
}

#[cfg(unix)]
#[test]
fn a_recent_program_never_becomes_the_default() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "true");
    fake_program(dir.path(), "tool", "true");
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        None,
        None,
        &[],
        &["tool --fast".into(), "claude".into()],
        None,
    );
    assert_eq!(list.default.as_deref(), Some("claude"));
}

#[cfg(unix)]
#[test]
fn the_default_is_the_project_program_only_while_it_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "true");
    fake_program(dir.path(), "tool", "true");
    let default_for = |project: &str| {
        crate::host::program::discover(
            Some(dir.path().as_os_str()),
            None,
            None,
            None,
            &[],
            &[project.to_owned()],
            Some(project),
        )
        .default
    };
    assert_eq!(default_for("tool --fast").as_deref(), Some("tool --fast"));
    assert_eq!(default_for("gone --fast").as_deref(), Some("claude"));
}

#[cfg(unix)]
#[test]
fn only_installed_known_agents_get_a_version() {
    let dir = tempfile::tempdir().unwrap();
    fake_program(dir.path(), "claude", "echo 'claude 9.9.9 (test)'");
    fake_program(dir.path(), "tool", "echo 'tool 1.0'");
    let mut list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        None,
        None,
        &[],
        &["tool".into()],
        None,
    );
    crate::host::program::versions(&mut list);
    let version = |command: &str| {
        list.programs
            .iter()
            .find(|p| p.command == command)
            .and_then(|p| p.version.clone())
    };
    assert_eq!(version("claude").as_deref(), Some("claude 9.9.9 (test)"));
    assert_eq!(version("tool"), None);
    assert_eq!(version("codex"), None);
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_projects_installed_program_is_its_new_thread_default(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let bin = fixture.dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    fake_program(&bin, "tool", "true");
    fake_program(&bin, "claude", "true");
    fixture.workspace.update(cx, |workspace, _| {
        workspace.use_program_environment(&bin, std::path::Path::new("/bin/sh"));
    });
    op(
        Op::Project(ProjectOp::SetProgram {
            id: fixture.project.clone(),
            program: Some("tool --fast".into()),
        }),
        cx,
    )
    .await
    .unwrap();
    let OpResult::Programs(list) = op(
        Op::Program(ProgramOp::Discover {
            project_id: Some(fixture.project.clone()),
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("programs")
    };
    assert_eq!(list.default.as_deref(), Some("tool --fast"));
    let missing = op(
        Op::Program(ProgramOp::Discover {
            project_id: Some(ProjectId("prj_missing".into())),
        }),
        cx,
    )
    .await
    .unwrap_err();
    assert!(matches!(missing, OpError::NotFound(_)), "{missing}");
}

#[test]
fn saved_programs_sit_after_the_shell_and_before_recent_ones_and_fold_a_repeat() {
    // Nothing is installed on this PATH, so the order does not depend on the machine.
    let dir = tempfile::tempdir().unwrap();
    let saved = |name: &str, line: &str, pinned: bool| kg_domain::SavedProgram {
        name: name.into(),
        line: line.into(),
        pinned,
    };
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        Some(std::ffi::OsStr::new("/bin/sh")),
        None,
        &[
            saved("Pinned", "tool --pinned", true),
            saved("Fast", "tool --fast", false),
            saved("Missing", "kg-no-such-program", false),
        ],
        &["tool --fast".into(), "tool --recent".into()],
        None,
    );
    let order: Vec<_> = list
        .programs
        .iter()
        .map(|program| (program.source, program.label.as_str()))
        .collect();
    let position = |label: &str| order.iter().position(|(_, found)| *found == label).unwrap();
    assert!(position("Pinned") < position("Fast"));
    assert!(
        position("Fast") < position("tool"),
        "recent ones follow the pinned ones"
    );
    assert!(position("tool") < position("Shell"), "then the rest");
    assert!(position("tool") < position("Missing"));
    assert_eq!(
        order
            .iter()
            .filter(|(source, _)| *source == kg_domain::ProgramSource::Saved)
            .map(|(_, label)| *label)
            .collect::<Vec<_>>(),
        ["Pinned", "Fast", "Missing"],
        "saved programs keep the order the store gives them"
    );
    let fast: Vec<_> = list
        .programs
        .iter()
        .filter(|program| program.command == "tool --fast")
        .collect();
    assert_eq!(
        fast.len(),
        1,
        "a saved line that was also recent is one row"
    );
    assert_eq!(fast[0].label, "Fast");
    let missing = list
        .programs
        .iter()
        .find(|program| program.label == "Missing")
        .unwrap();
    assert!(!missing.installed());
    assert!(list.programs.iter().any(|p| p.command == "tool --recent"));
}

#[test]
fn a_recent_command_is_named_by_its_program_and_keeps_its_flags() {
    let dir = tempfile::tempdir().unwrap();
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        None,
        None,
        &[],
        &[
            "cursor-agent --model grok-4.7-high --trust --force".into(),
            "/opt/tools/mytool --fast".into(),
        ],
        None,
    );
    let label = |command: &str| {
        list.programs
            .iter()
            .find(|p| p.command == command)
            .map(|p| p.label.clone())
    };
    assert_eq!(
        label("cursor-agent --model grok-4.7-high --trust --force").as_deref(),
        Some("Cursor Agent")
    );
    assert_eq!(label("/opt/tools/mytool --fast").as_deref(), Some("mytool"));
}

#[cfg(unix)]
#[test]
fn a_known_program_in_its_install_folder_is_found_without_the_login_path() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_program(&bin, "opencode", "true");
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let installed = |home: Option<&std::path::Path>| {
        crate::host::program::discover(Some(empty.as_os_str()), None, None, home, &[], &[], None)
            .programs
            .into_iter()
            .find(|p| p.command == "opencode")
            .and_then(|p| p.path)
    };
    assert_eq!(installed(Some(&home)), Some(bin.join("opencode")));
    assert_eq!(installed(None), None);
}

#[cfg(unix)]
#[test]
fn a_run_finds_a_known_program_in_its_install_folder() {
    use crate::host::program::ProgramEnvironment;
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_program(&bin, "opencode", "true");
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let environment = ProgramEnvironment {
        path: empty.as_os_str().to_owned(),
        shell: "/bin/sh".into(),
    };
    let found =
        crate::thread::resolve_program("opencode --model x", Some(&environment), Some(&home))
            .unwrap();
    assert_eq!(found.path, bin.join("opencode"));
    assert!(crate::thread::resolve_program("opencode", Some(&environment), None).is_err());
}

#[test]
fn a_recent_command_with_an_exe_suffix_is_named_by_its_program() {
    let dir = tempfile::tempdir().unwrap();
    let list = crate::host::program::discover(
        Some(dir.path().as_os_str()),
        None,
        None,
        None,
        &[],
        &["claude.exe --fast".into()],
        None,
    );
    let label = list
        .programs
        .iter()
        .find(|p| p.command == "claude.exe --fast")
        .map(|p| p.label.clone());
    assert_eq!(label.as_deref(), Some("Claude Code"));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_program_in_the_home_install_folder_is_listed_from_the_workspace_home(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new(cx).await;
    let dir = fixture.dir.path();
    let home = dir.join("home");
    let bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_program(&bin, "opencode", "true");
    let empty = dir.join("empty");
    std::fs::create_dir(&empty).unwrap();
    fixture.workspace.update(cx, |workspace, _| {
        workspace.use_program_environment(&empty, std::path::Path::new("/bin/sh"));
        workspace.homes.insert("HOME", home.clone());
    });
    let OpResult::Programs(list) = op(Op::Program(ProgramOp::Discover { project_id: None }), cx)
        .await
        .unwrap()
    else {
        panic!("programs")
    };
    let opencode = list
        .programs
        .iter()
        .find(|p| p.command == "opencode")
        .and_then(|p| p.path.clone());
    assert_eq!(opencode, Some(bin.join("opencode")));
}

#[cfg(unix)]
#[test]
fn more_documented_folders_find_cursor_goose_and_amp_without_the_login_path() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let folders = [
        ("cursor-agent", ".local/bin"),
        ("goose", ".local/bin"),
        ("amp", ".amp/bin"),
    ];
    for (command, folder) in folders {
        let bin = home.join(folder);
        std::fs::create_dir_all(&bin).unwrap();
        fake_program(&bin, command, "true");
    }
    let list = crate::host::program::discover(
        Some(empty.as_os_str()),
        None,
        None,
        Some(&home),
        &[],
        &[],
        None,
    );
    for (command, folder) in folders {
        let found = list
            .programs
            .iter()
            .find(|p| p.command == command)
            .and_then(|p| p.path.clone());
        assert_eq!(found, Some(home.join(folder).join(command)), "{command}");
    }
}

#[cfg(unix)]
#[test]
fn a_version_is_read_from_a_program_in_its_documented_folder() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    let bin = home.join(".opencode/bin");
    std::fs::create_dir_all(&bin).unwrap();
    fake_program(&bin, "opencode", "echo 'opencode 1.4.2'");
    let empty = dir.path().join("empty");
    std::fs::create_dir(&empty).unwrap();
    let mut list = crate::host::program::discover(
        Some(empty.as_os_str()),
        None,
        None,
        Some(&home),
        &[],
        &[],
        None,
    );
    crate::host::program::versions(&mut list);
    let version = list
        .programs
        .iter()
        .find(|p| p.command == "opencode")
        .and_then(|p| p.version.clone());
    assert_eq!(version.as_deref(), Some("opencode 1.4.2"));
}

#[cfg(unix)]
#[gpui_kit::test]
async fn a_thread_without_a_title_is_named_by_its_program_not_its_path(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let path = fixture.dir.path().join("codex");
    fake_program(fixture.dir.path(), "codex", "true");
    let line = format!("{} --fast", path.display());
    let OpResult::Thread(started) = op(
        Op::Thread(kg_domain::ThreadOp::Run(kg_domain::Run {
            project_id: fixture.project.clone(),
            program: line,
            title: None,
            environment: kg_domain::EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            quiet: true,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
        cx,
    )
    .await
    .unwrap() else {
        panic!("thread")
    };
    assert_eq!(started.title, "Codex");
}

#[cfg(unix)]
#[gpui_kit::test]
async fn removing_a_saved_program_waits_for_the_person(cx: &mut TestAppContext) {
    let _fixture = Fixture::new(cx).await;
    op(
        Op::Program(ProgramOp::Save {
            name: "Zed".into(),
            line: "tool --zed".into(),
        }),
        cx,
    )
    .await
    .unwrap();
    let removal = cx.update(|cx| {
        crate::Workspace::dispatch(Op::Program(ProgramOp::Remove { name: "Zed".into() }), cx)
    });
    cx.run_until_parked();
    assert_eq!(
        saved_names(cx).await,
        ["Zed"],
        "nothing is removed before the person answers"
    );
    let approval = cx.read(|cx| {
        crate::Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .unwrap()
            .id
            .clone()
    });
    cx.update(|cx| crate::Workspace::answer_standalone(approval, "approve".into(), cx))
        .await
        .unwrap();
    removal.await.unwrap();
    assert!(saved_names(cx).await.is_empty());
}

#[cfg(unix)]
async fn saved_names(cx: &mut TestAppContext) -> Vec<String> {
    let OpResult::Programs(list) = op(Op::Program(ProgramOp::List { project_id: None }), cx)
        .await
        .unwrap()
    else {
        panic!("programs")
    };
    list.programs
        .into_iter()
        .filter(|program| program.source == kg_domain::ProgramSource::Saved)
        .map(|program| program.label)
        .collect()
}
