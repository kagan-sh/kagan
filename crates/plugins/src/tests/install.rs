use std::path::Path;
use std::process::Command;
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpError, OpResult, Plugin, PluginId, PluginOp, ThreadOp};
use serde_json::json;

use crate::PluginHostGlobal;
use crate::manifest::Grant;
use crate::source;

use super::{
    ID, activation_of, answer, answer_standalone, assert_folder_did_not_land, assert_folder_landed,
    call, git, hold_activation, hold_commits, hold_lifecycle, hold_writes, install, installed,
    installing_now, loaded, open, open_kept, plugin_dir, plugin_op, queue_folder, records, run,
    sample_manifest, save_notes_now, stored_value, thread, tool_names, unique, write_plugin,
};

#[cfg(unix)]
#[test]
fn git_fetch_respects_proxy_and_ssh_settings() {
    use std::os::unix::fs::PermissionsExt as _;

    if let Some(case) = std::env::var_os("KAGAN_FETCH_CASE") {
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("called");
        let source = source::Source::parse(&std::env::var("KAGAN_FETCH_SOURCE").unwrap()).unwrap();
        let (_keep, stop) = async_channel::bounded(1);
        let result = source::fetch(&source, None, dir.path(), dir.path(), &stop);
        if case == "unreachable" {
            assert!(
                matches!(result, Err(OpError::Failed(message)) if message.contains("could not reach 127.0.0.1"))
            );
            assert!(!marker.exists());
        } else {
            assert!(
                matches!(result, Err(OpError::Failed(message)) if message.contains("git failed"))
            );
        }
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let git = dir.path().join("git");
    let log = dir.path().join("git-log");
    std::fs::write(&git, "#!/bin/sh\nif [ \"$1\" = config ]; then\n  case \"$3\" in http.proxy) printf '%s' \"$KAGAN_TEST_PROXY\";; core.sshCommand) printf '%s' \"$KAGAN_TEST_SSH\";; esac\n  exit 0\nfi\nprintf '%s' \"${GIT_SSH_COMMAND-unset}\" > \"$KAGAN_TEST_LOG\"\necho fake >&2\nexit 1\n").unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let https = format!("git:https://127.0.0.1:{port}/repo@main");
    let run = |case: &str, source: &str, vars: &[(&str, &str)], expected: Option<&str>| {
        let _ = std::fs::remove_file(&log);
        let mut child = Command::new(std::env::current_exe().unwrap());
        child
            .args([
                "--exact",
                "tests::install::git_fetch_respects_proxy_and_ssh_settings",
                "--nocapture",
            ])
            .env_clear()
            .env("PATH", dir.path())
            .env("HOME", dir.path())
            .env("KAGAN_FETCH_CASE", case)
            .env("KAGAN_FETCH_SOURCE", source)
            .env("KAGAN_TEST_LOG", &log);
        for (key, value) in vars {
            child.env(key, value);
        }
        let output = child.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
        if let Some(expected) = expected {
            assert_eq!(std::fs::read_to_string(&log).unwrap(), expected);
        } else {
            assert!(!log.exists());
        }
    };
    run("unreachable", &https, &[("HTTPS_PROXY", "")], None);
    run("unreachable", &https, &[("KAGAN_TEST_PROXY", "")], None);
    for key in [
        "HTTPS_PROXY",
        "https_proxy",
        "HTTP_PROXY",
        "http_proxy",
        "ALL_PROXY",
        "all_proxy",
    ] {
        run("proxy", &https, &[(key, "proxy")], Some("unset"));
    }
    run(
        "proxy",
        &https,
        &[("KAGAN_TEST_PROXY", "proxy")],
        Some("unset"),
    );
    let ssh = "git:ssh://example.invalid/repo@main";
    run("ssh", ssh, &[], Some("ssh -o ConnectTimeout=15"));
    run("ssh", ssh, &[("GIT_SSH_COMMAND", "custom")], Some("custom"));
    run("ssh", ssh, &[("KAGAN_TEST_SSH", "custom")], Some("unset"));
}

#[gpui_kit::test]
fn an_install_or_removal_whose_command_stopped_never_acts(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let source = plugin.path().display().to_string();
    open(cx, &[]);
    let approvals =
        |cx: &mut TestAppContext| cx.read(|cx| Workspace::global(cx).read(cx).approvals().count());
    let install_for = |cx: &mut TestAppContext, gone| {
        let op = Op::Plugin(install(source.clone(), None, None));
        let task =
            cx.update(|cx| Workspace::dispatch_for(op, Some(kg_core::Requester::new(gone)), cx));
        cx.run_until_parked();
        task
    };

    let (present, gone) = async_channel::bounded::<()>(1);
    let stopped = install_for(cx, gone);
    assert_eq!(approvals(cx), 1);
    drop(present);
    cx.run_until_parked();
    assert_eq!(
        approvals(cx),
        0,
        "the card stayed after its command stopped"
    );
    assert!(cx.foreground_executor().block_test(stopped).is_err());
    assert!(records(cx).is_empty());

    let (_present, gone) = async_channel::bounded::<()>(1);
    let approved = install_for(cx, gone);
    answer_standalone(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(approved).unwrap(),
        OpResult::Done
    );
    assert_eq!(records(cx).len(), 1);

    let (present, gone) = async_channel::bounded::<()>(1);
    let removing = cx.update(|cx| {
        Workspace::dispatch_for(
            Op::Plugin(PluginOp::Remove {
                id: PluginId(ID.into()),
                thread_id: None,
            }),
            Some(kg_core::Requester::new(gone)),
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(approvals(cx), 1);
    drop(present);
    cx.run_until_parked();
    assert_eq!(
        approvals(cx),
        0,
        "the card stayed after its command stopped"
    );
    assert!(cx.foreground_executor().block_test(removing).is_err());
    assert_eq!(records(cx).len(), 1);
}

#[gpui_kit::test]
fn installing_needs_a_person_with_or_without_a_thread(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let source = plugin.path().display().to_string();
    open(cx, &[]);

    let standalone = plugin_op(cx, install(source.clone(), None, None));
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .unwrap()
    });
    assert!(approval.title.contains("writes to your clipboard"));
    let invalid =
        cx.update(|cx| Workspace::answer_standalone(approval.id.clone(), "invented".into(), cx));
    assert!(cx.foreground_executor().block_test(invalid).is_err());
    assert_eq!(
        cx.read(|cx| Workspace::global(cx).read(cx).approvals().count()),
        1
    );
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(standalone).is_err());
    assert!(records(cx).is_empty());

    let declined = plugin_op(cx, install(source.clone(), Some(thread()), None));
    let title = answer(cx, "decline");
    assert_eq!(
        title,
        format!(
            "Install Echo 1.0.0 ({ID})?\nFrom the folder {}.\nCaution: a folder on this computer is not pinned to a commit.\nIt:\n- writes to your clipboard\n- reads a folder you choose (Notes)",
            dunce::canonicalize(plugin.path()).unwrap().display()
        )
    );
    assert!(cx.foreground_executor().block_test(declined).is_err());
    assert!(records(cx).is_empty());
    assert_eq!(tool_names(cx).1, ["skill_load"]);

    let approved = plugin_op(cx, install(source, Some(thread()), None));
    answer(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(approved).unwrap(),
        OpResult::Done
    );
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(record.root, dunce::canonicalize(plugin.path()).unwrap());
    assert!(record.enabled && record.commit.is_none());
    assert_eq!(
        tool_names(cx).1,
        ["echo", "broken", "hang", "peek", "skill_load"]
    );
}

#[gpui_kit::test]
fn git_installs_lock_to_a_commit_and_updates_ask_only_for_new_powers(cx: &mut TestAppContext) {
    let repo = tempfile::tempdir().unwrap();
    git(repo.path(), &["init", "--quiet", "--initial-branch=main"]);
    write_plugin(&repo.path().join("echo"), &sample_manifest());
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "--quiet", "-m", "one"]);
    let first = git(repo.path(), &["rev-parse", "HEAD"]);
    let source = format!("git:{}@main", repo.path().display());
    open(cx, &[]);

    let installing = plugin_op(cx, install(source.clone(), Some(thread()), Some("echo")));
    answer(cx, "approve");
    cx.foreground_executor().block_test(installing).unwrap();
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(record.commit.as_deref(), Some(first.as_str()));
    assert!(record.root.ends_with(Path::new(&first).join("echo")));
    assert_eq!(record.source, source);
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );

    let update = |thread_id| PluginOp::Update {
        id: PluginId(ID.into()),
        thread_id,
    };
    assert_eq!(
        run(cx, Op::Plugin(update(None))).unwrap(),
        OpResult::Done,
        "the same commit needs no approval"
    );

    let mut grown = sample_manifest();
    grown["version"] = json!("1.1.0");
    grown["powers"]["run"] = json!([{ "program": "gh" }]);
    write_plugin(&repo.path().join("echo"), &grown);
    git(repo.path(), &["commit", "--quiet", "-am", "two"]);
    let second = git(repo.path(), &["rev-parse", "HEAD"]);

    let declined = plugin_op(cx, update(None));
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());
    assert_eq!(records(cx)[0].commit.as_deref(), Some(first.as_str()));

    let updating = plugin_op(cx, update(Some(thread())));
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Update Echo to 1.1.0 ({ID})?\nFrom {} at main, commit {second}.\nCaution: this repository is on this computer, not where an index you added lives.\nIt:\n- runs gh",
            repo.path().display()
        )
    );
    cx.foreground_executor().block_test(updating).unwrap();
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(record.commit.as_deref(), Some(second.as_str()));
    assert!(!loaded(cx), "an update drops the old load");
}

#[test]
fn git_sources_need_a_ref() {
    for text in ["git:git@github.com:o/r", "git:https://example.com/r"] {
        let Err(error) = source::Source::parse(text) else {
            panic!("{text} parsed")
        };
        assert!(error.to_string().contains("needs a ref @<ref>"), "{error}");
    }
    assert!(source::Source::parse("git:git@github.com:o/r@v1").is_ok());
}

#[gpui_kit::test]
fn powers_that_grow_on_disk_are_not_loaded_until_approved(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = || PluginId(ID.into());

    let mut grown = sample_manifest();
    grown["powers"]["clipboard"]["read"] = json!(true);
    grown["powers"]["fetch"] = json!([{ "host": "api.github.com", "methods": ["POST"] }]);
    write_plugin(plugin.path(), &grown);
    run(cx, Op::Plugin(PluginOp::Disable { id: id() })).unwrap();
    let error = run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("needs approval for: talks to api.github.com (POST), reads your clipboard"),
        "{error}"
    );
    assert_eq!(tool_names(cx).1, ["skill_load"]);
    assert!(matches!(
        call(cx, "echo", json!({ "text": "hi" })),
        Err(OpError::NotFound(_))
    ));

    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: id(),
            thread_id: Some(thread()),
        },
    );
    let root = records(cx)[0].root.clone();
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Update Echo to 1.0.0 ({ID})?\nFrom the folder {}.\nCaution: a folder on this computer is not pinned to a commit.\nIt:\n- talks to api.github.com (POST)\n- reads your clipboard",
            root.display()
        )
    );
    cx.foreground_executor().block_test(updating).unwrap();
    run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap();
    assert_eq!(
        tool_names(cx).1,
        ["echo", "broken", "hang", "peek", "skill_load"]
    );
}

#[gpui_kit::test]
fn a_restart_does_not_load_powers_nobody_approved(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut record = installed(plugin.path());
    record.approved = serde_json::to_value(Grant::default()).unwrap();
    open(cx, &[record]);
    assert_eq!(tool_names(cx).1, ["skill_load"]);
    assert!(!loaded(cx));
}

#[gpui_kit::test]
fn a_folder_setting_is_approved_by_a_person(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    open(cx, &[installed(plugin.path())]);
    let set = |thread_id| PluginOp::Config {
        id: PluginId(ID.into()),
        op: ConfigOp::Set {
            key: "notes".into(),
            value: Some(json!(notes.path().display().to_string())),
        },
        thread_id,
    };

    let declined = plugin_op(cx, set(None));
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());

    let setting = plugin_op(cx, set(Some(thread())));
    let path = dunce::canonicalize(notes.path()).unwrap();
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Let Echo ({ID}) read a folder?\nIt:\n- reads {} (Notes)",
            path.display()
        )
    );
    cx.foreground_executor().block_test(setting).unwrap();
    assert_eq!(
        run(
            cx,
            Op::Plugin(PluginOp::Config {
                id: PluginId(ID.into()),
                op: ConfigOp::Get {
                    key: "notes".into()
                },
                thread_id: None,
            })
        )
        .unwrap(),
        OpResult::Setting(json!(path.display().to_string()))
    );
}

#[gpui_kit::test]
fn an_update_that_adds_a_read_folder_asks_again(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let folder = format!("~/{}", unique("added"));
    let mut grown = sample_manifest();
    grown["powers"]["read"] = json!([folder]);
    write_plugin(plugin.path(), &grown);
    let id = || PluginId(ID.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: id() })).unwrap();
    let error = run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap_err();
    assert!(
        error.to_string().contains(&format!("reads {folder}")),
        "{error}"
    );
    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: id(),
            thread_id: Some(thread()),
        },
    );
    let root = records(cx)[0].root.clone();
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Update Echo to 1.0.0 ({ID})?\nFrom the folder {}.\nCaution: a folder on this computer is not pinned to a commit.\nIt:\n- reads {folder}",
            root.display()
        )
    );
    cx.foreground_executor().block_test(updating).unwrap();
}

#[gpui_kit::test]
fn disabling_while_a_folder_waits_for_approval_keeps_it_disabled(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    open(cx, &[installed(plugin.path())]);

    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    answer(cx, "approve");
    let error = cx.foreground_executor().block_test(setting).unwrap_err();
    assert!(
        error.to_string().contains("changed while this was saving"),
        "{error}"
    );

    let [record] = records(cx).try_into().unwrap();
    assert!(
        !record.enabled,
        "the approval must not re-enable the plugin"
    );
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert!(
        !approved.paths.contains_key("notes"),
        "a disabled plugin does not take the folder that was still saving"
    );
    assert_eq!(stored_value(cx, "notes"), None);
}

#[gpui_kit::test]
fn a_save_held_across_reload_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
    let release = hold_writes(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "sounds".into(),
                value: Some(json!(false)),
            },
            thread_id: None,
        },
    );
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(reloading).unwrap();
    assert_ne!(activation_of(cx), before);
    let error = cx.foreground_executor().block_test(setting).unwrap_err();
    assert!(
        error.to_string().contains("changed while this was saving"),
        "{error}"
    );
    assert_eq!(
        run(
            cx,
            Op::Plugin(PluginOp::Config {
                id: PluginId(ID.into()),
                op: ConfigOp::Get {
                    key: "sounds".into(),
                },
                thread_id: None,
            }),
        )
        .unwrap(),
        OpResult::Setting(json!(true))
    );
    assert_eq!(stored_value(cx, "sounds"), None);
}

#[gpui_kit::test]
fn a_save_held_across_disable_and_enable_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let release = hold_writes(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "sounds".into(),
                value: Some(json!(false)),
            },
            thread_id: None,
        },
    );
    let disabling = plugin_op(
        cx,
        PluginOp::Disable {
            id: PluginId(ID.into()),
        },
    );
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(disabling).unwrap();
    cx.foreground_executor().block_test(enabling).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_eq!(stored_value(cx, "sounds"), None);
    assert_eq!(
        run(
            cx,
            Op::Plugin(PluginOp::Config {
                id: PluginId(ID.into()),
                op: ConfigOp::Get {
                    key: "sounds".into(),
                },
                thread_id: None,
            }),
        )
        .unwrap(),
        OpResult::Setting(json!(true))
    );
}

#[gpui_kit::test]
fn a_folder_save_queued_across_reload_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, setting, _notes) = queue_folder(cx);
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(reloading).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_folder_did_not_land(cx);
}

#[gpui_kit::test]
fn a_failed_update_leaves_a_running_plugin_active(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
    assert!(before.is_some());
    let removed = cx.update(|cx| {
        Workspace::global(cx)
            .read(cx)
            .remove_plugin(PluginId(ID.into()))
    });
    cx.foreground_executor().block_test(removed).unwrap();
    let error = run(
        cx,
        Op::Plugin(PluginOp::Update {
            id: PluginId(ID.into()),
            thread_id: None,
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("plugin was removed"), "{error}");
    assert_eq!(activation_of(cx), before);
}

#[gpui_kit::test]
fn a_folder_save_queued_across_update_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, setting, _notes) = queue_folder(cx);
    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(updating).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_folder_did_not_land(cx);
}

#[gpui_kit::test]
fn a_folder_save_queued_across_disable_and_enable_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, setting, _notes) = queue_folder(cx);
    let disabling = plugin_op(
        cx,
        PluginOp::Disable {
            id: PluginId(ID.into()),
        },
    );
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(disabling).unwrap();
    cx.foreground_executor().block_test(enabling).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    assert_folder_did_not_land(cx);
    let [record] = records(cx).try_into().unwrap();
    assert!(record.enabled);
}

#[gpui_kit::test]
fn a_folder_save_on_the_new_activation_survives_enable(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut installed = installed(plugin.path());
    installed.enabled = false;
    open(cx, &[installed]);
    let release = hold_activation(cx);
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    let (_notes, path) = save_notes_now(cx);
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(enabling).unwrap();
    assert_folder_landed(cx, &path);
    let [record] = records(cx).try_into().unwrap();
    assert!(record.enabled);
}

#[gpui_kit::test]
fn a_folder_save_on_the_new_activation_survives_reload(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
    let release = hold_activation(cx);
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    assert_ne!(activation_of(cx), before);
    let (_notes, path) = save_notes_now(cx);
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(reloading).unwrap();
    assert_folder_landed(cx, &path);
}

#[gpui_kit::test]
fn a_folder_save_on_the_new_activation_survives_disable(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let release = hold_lifecycle(cx);
    let disabling = plugin_op(
        cx,
        PluginOp::Disable {
            id: PluginId(ID.into()),
        },
    );
    let enabling = plugin_op(
        cx,
        PluginOp::Enable {
            id: PluginId(ID.into()),
        },
    );
    cx.foreground_executor().block_test(enabling).unwrap();
    let (_notes, path) = save_notes_now(cx);
    release.try_send(()).unwrap();
    cx.foreground_executor().block_test(disabling).unwrap();
    assert_folder_landed(cx, &path);
}

#[gpui_kit::test]
fn a_folder_save_held_across_remove_and_reinstall_does_not_persist(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    let path = dunce::canonicalize(notes.path()).unwrap();
    open(cx, &[installed(plugin.path())]);
    let release = hold_commits(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    answer(cx, "approve");
    cx.run_until_parked();
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: Some(thread()),
        },
    );
    answer(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    release.try_send(()).unwrap();
    let installing = plugin_op(
        cx,
        install(plugin.path().display().to_string(), Some(thread()), None),
    );
    answer(cx, "approve");
    cx.foreground_executor().block_test(installing).unwrap();
    assert!(cx.foreground_executor().block_test(setting).is_err());
    let [record] = records(cx).try_into().unwrap();
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert_ne!(approved.paths.get("notes"), Some(&path));
    assert_eq!(stored_value(cx, "notes"), None);
}

#[gpui_kit::test]
fn a_folder_save_keeps_the_activation_and_a_decline_stores_nothing(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    open(cx, &[installed(plugin.path())]);
    let before = activation_of(cx);
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    answer(cx, "approve");
    cx.foreground_executor().block_test(setting).unwrap();
    assert_eq!(activation_of(cx), before);
    assert!(stored_value(cx, "notes").is_some());

    let again = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    answer(cx, "decline");
    assert!(cx.foreground_executor().block_test(again).is_err());
    assert_eq!(activation_of(cx), before);
}

#[gpui_kit::test]
fn a_folder_approved_while_an_update_waits_is_kept(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let notes = tempfile::tempdir().unwrap();
    let path = dunce::canonicalize(notes.path()).unwrap();
    open(cx, &[installed(plugin.path())]);

    let mut grown = sample_manifest();
    grown["powers"]["run"] = json!([{ "program": "gh" }]);
    write_plugin(plugin.path(), &grown);
    let updating = plugin_op(
        cx,
        PluginOp::Update {
            id: PluginId(ID.into()),
            thread_id: Some(thread()),
        },
    );
    let setting = plugin_op(
        cx,
        PluginOp::Config {
            id: PluginId(ID.into()),
            op: ConfigOp::Set {
                key: "notes".into(),
                value: Some(json!(notes.path().display().to_string())),
            },
            thread_id: Some(thread()),
        },
    );
    let Ok(OpResult::Interactions(pending)) =
        run(cx, Op::Thread(ThreadOp::Interactions { id: thread() }))
    else {
        panic!("interactions")
    };
    let folder = pending
        .iter()
        .find(|interaction| interaction.title.contains("read a folder"))
        .unwrap();
    let update = pending
        .iter()
        .find(|interaction| interaction.title.starts_with("Update"))
        .unwrap();
    for interaction in [folder, update] {
        let answered = cx.update(|cx| {
            Workspace::answer(&thread(), interaction.id.clone(), "approve".into(), cx)
        });
        cx.run_until_parked();
        cx.foreground_executor().block_test(answered).unwrap();
    }
    cx.foreground_executor().block_test(setting).unwrap();
    cx.foreground_executor().block_test(updating).unwrap();

    let [record] = records(cx).try_into().unwrap();
    let approved: Grant = serde_json::from_value(record.approved).unwrap();
    assert_eq!(approved.powers.run.len(), 1, "the update committed");
    assert_eq!(approved.paths.get("notes"), Some(&path));
}

#[gpui_kit::test]
fn reload_and_dev_wait_for_app_approval_for_added_powers(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let mut changed = sample_manifest();
    changed["powers"]["clipboard"]["read"] = true.into();
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        changed.to_string(),
    )
    .unwrap();
    let reload = |thread_id| PluginOp::Reload {
        id: PluginId(ID.into()),
        thread_id,
    };
    let declined = plugin_op(cx, reload(None));
    let approval = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .approvals()
            .next()
            .cloned()
            .expect("a standalone approval")
    });
    assert!(approval.title.contains("reads your clipboard"));
    assert_eq!(approval.options[0].label, "Cancel");
    answer_standalone(cx, "decline");
    assert!(cx.foreground_executor().block_test(declined).is_err());
    let pending = plugin_op(cx, reload(Some(thread())));
    assert!(answer(cx, "decline").contains("reads your clipboard"));
    assert!(cx.foreground_executor().block_test(pending).is_err());
    let pending = plugin_op(
        cx,
        PluginOp::Dev {
            path: plugin.path().into(),
            thread_id: Some(thread()),
        },
    );
    assert!(answer(cx, "approve").contains("reads your clipboard"));
    assert!(matches!(
        cx.foreground_executor().block_test(pending).unwrap(),
        OpResult::Plugins(_)
    ));
    assert_eq!(run(cx, Op::Plugin(reload(None))).unwrap(), OpResult::Done);
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    assert!(
        plugin.path().join("main.js").exists(),
        "removal keeps the local source"
    );
    assert_eq!(
        run(cx, Op::Plugin(PluginOp::List)).unwrap(),
        OpResult::Plugins(Vec::new())
    );
}

#[gpui_kit::test]
fn removing_a_plugin_waits_for_a_person(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let remove = |thread_id| PluginOp::Remove {
        id: PluginId(ID.into()),
        thread_id,
    };
    let removing = plugin_op(cx, remove(Some(thread())));
    assert_eq!(answer(cx, "decline"), format!("Remove “{ID}”?"));
    let error = cx.foreground_executor().block_test(removing).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("declined: Remove “{ID}”?")),
        "{error}"
    );
    assert_eq!(records(cx).len(), 1);

    let removing = plugin_op(cx, remove(Some(thread())));
    answer(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(removing).unwrap(),
        OpResult::Done
    );
    assert!(records(cx).is_empty());
}

#[gpui_kit::test]
fn removing_a_plugin_defers_and_retries_store_delete_past_the_drop_turn(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;

    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let dropped = Rc::new(Cell::new(false));
    let mark_drop = dropped.clone();
    cx.update(|cx| {
        let host = crate::PluginHost::global(cx);
        host.update(cx, |host, _| {
            host.set_drop_pages(Rc::new(move |_, _, _| {
                mark_drop.set(true);
            }));
        });
    });
    let attempts = Rc::new(Cell::new(0));
    let count = attempts.clone();
    let _hook = crate::web_store::set_remove_hook(move |_| {
        let n = count.get() + 1;
        count.set(n);
        if n < 3 {
            crate::web_store::StoreAttempt::InUse
        } else {
            crate::web_store::StoreAttempt::Removed
        }
    });
    let data = cx.update(|cx| {
        crate::PluginHost::global(cx)
            .read(cx)
            .plugin_data(&PluginId(ID.into()))
    });
    crate::keep_store(&data, "board").expect("store");
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.run_until_parked();
    assert!(dropped.get(), "pages drop in the remove turn");
    assert_eq!(
        attempts.get(),
        0,
        "remove_data_store waits for the native release after drop"
    );
    assert!(
        crate::store_dir(&data, "board").join("store.id").is_file(),
        "store.id stays until WebKit accepts the delete"
    );
    cx.executor().advance_clock(
        crate::web_store::RELEASE_WAIT
            .checked_sub(Duration::from_millis(1))
            .expect("release wait"),
    );
    cx.run_until_parked();
    assert_eq!(attempts.get(), 0, "still inside the release wait");
    cx.executor().advance_clock(Duration::from_millis(1));
    cx.run_until_parked();
    assert_eq!(attempts.get(), 1, "first attempt after the release wait");
    assert!(
        crate::store_dir(&data, "board").join("store.id").is_file(),
        "InUse keeps store.id for a retry"
    );
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert_eq!(
        cx.foreground_executor().block_test(removing).unwrap(),
        OpResult::Done
    );
    assert_eq!(attempts.get(), 3, "retries InUse until Removed");
    assert!(
        !crate::store_dir(&data, "board").join("store.id").is_file(),
        "store.id clears only after WebKit accepts the delete"
    );
}

#[gpui_kit::test]
fn reload_stops_when_the_plugin_was_removed(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let (release, hold) = async_channel::bounded(1);
    cx.update(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.clone();
        host.update(cx, |host, _| host.keep_hold = Some(hold));
    });
    let reloading = plugin_op(
        cx,
        PluginOp::Reload {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.foreground_executor().block_test(removing).unwrap();
    release.try_send(()).unwrap();
    assert_eq!(
        cx.foreground_executor()
            .block_test(reloading)
            .unwrap_err()
            .to_string(),
        "plugin was removed"
    );
    assert!(records(cx).is_empty());
}

#[gpui_kit::test]
fn cancel_stops_a_git_install_and_leaves_nothing_staged(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);
    // A server that takes the connection and never answers, so git waits on it.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming().flatten() {
            held.push(stream);
        }
    });
    let install = cx.update(|cx| {
        Workspace::dispatch(
            Op::Plugin(PluginOp::Install {
                source: format!("git:http://127.0.0.1:{port}/plugin.git@main"),
                plugin: None,
                thread_id: None,
            }),
            cx,
        )
    });
    // The test executor runs the git step on this thread, where it would wait on the server for
    // ever. So run one task at a time until the install can be stopped, then run Cancel at once:
    // it acts on its first poll, before the git step gets the thread.
    let host = cx.update(|cx| crate::PluginHost::global(cx));
    while !cx.read(|cx| host.read(cx).stoppable()) {
        assert!(
            cx.executor().tick(),
            "the install never reached its git step"
        );
    }
    let mut async_cx = cx.to_async();
    let cancelled = futures::FutureExt::now_or_never(crate::ops::run(
        &host,
        PluginOp::CancelInstall,
        None,
        &mut async_cx,
    ))
    .expect("Cancel answers at once");
    assert!(matches!(cancelled, Ok(OpResult::Done)), "{cancelled:?}");
    let error = cx.foreground_executor().block_test(install).unwrap_err();
    assert_eq!(error, OpError::Failed("the install was stopped".into()));
    let plugins = path.join("plugins");
    let left = std::fs::read_dir(&plugins).map_or(0, Iterator::count);
    assert_eq!(
        left,
        0,
        "no staging folder is left in {}",
        plugins.display()
    );
}

#[gpui_kit::test]
fn a_plugin_that_did_not_start_says_why_when_listed(cx: &mut TestAppContext) {
    // An install from before the refusal: its manifest still declares a toolbar.
    let plugin = plugin_dir();
    let mut old = sample_manifest();
    old["ui"] = json!({"toolbar": [{"id": "canvas-open", "view": "CanvasButton"}]});
    std::fs::write(plugin.path().join(crate::manifest::FILE), old.to_string()).unwrap();
    let root = dunce::canonicalize(plugin.path()).unwrap();
    let record = Plugin {
        id: PluginId(ID.into()),
        source: root.display().to_string(),
        approved: json!({"powers": {}, "downloads": {}}),
        root,
        commit: None,
        enabled: true,
        error: None,
    };
    open(cx, &[record]);
    let OpResult::Plugins(list) = run(cx, Op::Plugin(PluginOp::List)).unwrap() else {
        panic!("list returned no plugins");
    };
    let error = list[0]
        .error
        .as_deref()
        .expect("the start failure is listed");
    assert!(error.contains("toolbar"), "{error}");
}

#[gpui_kit::test]
fn a_second_install_is_refused_while_one_waits_for_approval(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[]);
    let source = plugin.path().display().to_string();
    let first = plugin_op(cx, install(source.clone(), None, None));
    let second = plugin_op(cx, install(source.clone(), None, None));
    let error = cx
        .foreground_executor()
        .block_test(second)
        .unwrap_err()
        .to_string();
    assert!(error.contains("already installing"), "{error}");
    answer_standalone(cx, "approve");
    assert_eq!(
        cx.foreground_executor().block_test(first).unwrap(),
        OpResult::Done
    );
    assert!(installing_now(cx).is_none(), "the slot is free again");
}

#[gpui_kit::test]
fn a_failed_store_delete_keeps_its_id_and_says_why(cx: &mut TestAppContext) {
    let _hook = crate::web_store::set_remove_hook(|_| {
        crate::web_store::StoreAttempt::Failed("the store is locked".into())
    });
    let mut async_cx = cx.to_async();
    let removal = cx
        .foreground_executor()
        .block_test(crate::web_store::remove_web_stores(
            vec![[7; 16]],
            &mut async_cx,
        ));
    assert!(removal.removed.is_empty(), "a failed delete keeps store.id");
    assert_eq!(removal.warnings, vec!["the store is locked".to_owned()]);
}
