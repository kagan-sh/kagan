use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{
    ConfigOp, EnvironmentSpec, KeymapOp, Op, OpError, OpResult, PluginId, PluginOp, ProjectId, Run,
    ThreadOp, ThreadStatus,
};
use serde_json::{Value, json};

use crate::PluginHostGlobal;

use super::{
    AREAS_PROBE, COMMAND_PROBE, ID, MAIN, RENAME_PROBE, THREAD, THREAD_OPS, THREADS_FORWARD,
    answered, call, call_and_approve, dispatched, forward, forwarded, forwarded_reply, forwarding,
    hides_agent, installed, installed_example, loaded, open, pick_worker, plugin_dir, probe_plugin,
    real_call, run, sample_manifest, settled, shell, sleeping, thread, write_plugin,
};
#[cfg(unix)]
use super::{alive, start_tree};

#[gpui_kit::test]
fn settings_are_typed_by_the_manifest(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let config = |op| {
        Op::Plugin(PluginOp::Config {
            id: PluginId(ID.into()),
            op,
            thread_id: None,
        })
    };

    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "sounds".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!(true))
    );
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "retries".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!(3))
    );
    let error = run(
        cx,
        config(ConfigOp::Set {
            key: "sounds".into(),
            value: Some(json!("loud")),
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("takes true or false"), "{error}");
    let number_error = run(
        cx,
        config(ConfigOp::Set {
            key: "retries".into(),
            value: Some(json!("three")),
        }),
    )
    .unwrap_err();
    assert!(
        number_error
            .to_string()
            .contains("takes a number, like 3 or 2.5"),
        "{number_error}"
    );
    assert!(matches!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "missing".into()
            })
        ),
        Err(OpError::Invalid(_))
    ));
    run(
        cx,
        config(ConfigOp::Set {
            key: "sounds".into(),
            value: Some(json!(false)),
        }),
    )
    .unwrap();
    run(
        cx,
        config(ConfigOp::Set {
            key: "retries".into(),
            value: Some(json!(2.5)),
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "retries".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!(2.5))
    );
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi false"),
            is_error: false
        }
    );
}

#[gpui_kit::test]
fn a_removed_project_keeps_its_stored_id(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    let config = |op| {
        Op::Plugin(PluginOp::Config {
            id: id.clone(),
            op,
            thread_id: None,
        })
    };
    run(
        cx,
        config(ConfigOp::Set {
            key: "board".into(),
            value: Some(json!("prj_test")),
        }),
    )
    .unwrap();
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "board", &json!("prj_gone"))
            .unwrap();
    });
    run(
        cx,
        Op::Plugin(PluginOp::Reload {
            id: id.clone(),
            thread_id: None,
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!("prj_gone"))
    );
}

#[gpui_kit::test]
fn a_stored_project_id_loads_only_as_text_or_null(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    let get = Op::Plugin(PluginOp::Config {
        id: id.clone(),
        op: ConfigOp::Get {
            key: "board".into(),
        },
        thread_id: None,
    });
    let reload = Op::Plugin(PluginOp::Reload {
        id: id.clone(),
        thread_id: None,
    });
    for old in [json!(true), json!(3), json!({ "id": "prj_kagan" })] {
        cx.update(|cx| {
            let data = Workspace::global(cx).read(cx).data_dir().to_owned();
            let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
            store.set_plugin_value(&id, "board", &old).unwrap();
        });
        run(cx, reload.clone()).unwrap();
        assert_eq!(
            run(cx, get.clone()).unwrap(),
            OpResult::Setting(Value::Null)
        );
    }
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "board", &json!("prj_gone"))
            .unwrap();
    });
    run(cx, reload).unwrap();
    assert_eq!(run(cx, get).unwrap(), OpResult::Setting(json!("prj_gone")));
}

#[gpui_kit::test]
fn a_stored_number_coerces_or_falls_back_on_load(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let id = PluginId(ID.into());
    let get = Op::Plugin(PluginOp::Config {
        id: id.clone(),
        op: ConfigOp::Get {
            key: "retries".into(),
        },
        thread_id: None,
    });
    let reload = Op::Plugin(PluginOp::Reload {
        id: id.clone(),
        thread_id: None,
    });
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "retries", &json!("30"))
            .unwrap();
    });
    run(cx, reload.clone()).unwrap();
    assert_eq!(run(cx, get.clone()).unwrap(), OpResult::Setting(json!(30)));
    cx.update(|cx| {
        let data = Workspace::global(cx).read(cx).data_dir().to_owned();
        let store = kg_store::Store::open(data.join("kagan.sqlite3")).unwrap();
        store
            .set_plugin_value(&id, "retries", &json!(true))
            .unwrap();
    });
    run(cx, reload).unwrap();
    assert_eq!(run(cx, get).unwrap(), OpResult::Setting(json!(3)));
}

#[gpui_kit::test]
fn native_modules_exist_only_when_their_areas_are_granted(cx: &mut TestAppContext) {
    for area in ["process", "threads"] {
        let plugin = plugin_dir();
        let import = format!("import * as native from 'kagan/{area}';\n");
        std::fs::write(plugin.path().join("main.js"), format!("{import}{MAIN}")).unwrap();
        open(cx, &[installed(plugin.path())]);
        let error = call(cx, "echo", json!({ "text": "hi" })).unwrap_err();
        assert!(
            error.to_string().contains(&format!("kagan/{area}")),
            "{error}"
        );
        assert!(!loaded(cx));

        let mut declared = sample_manifest();
        declared["powers"]["areas"] = json!([area]);
        std::fs::write(
            plugin.path().join(crate::manifest::FILE),
            declared.to_string(),
        )
        .unwrap();
        open(cx, &[installed(plugin.path())]);
        assert!(matches!(
            call(cx, "echo", json!({ "text": "hi" })),
            Ok(OpResult::ToolReply {
                is_error: false,
                ..
            })
        ));
    }
}

#[gpui_kit::test]
fn a_plugin_rename_keeps_a_locked_title(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads"], RENAME_PROBE);
    open(cx, &[installed(plugin.path())]);
    let OpResult::Thread(named) = run(
        cx,
        Op::Thread(ThreadOp::Rename {
            id: thread(),
            title: "Named".into(),
        }),
    )
    .unwrap() else {
        panic!("window rename")
    };
    assert!(named.title_locked);
    let OpResult::ToolReply { text, .. } = real_call(cx, "probe").unwrap() else {
        panic!("tool reply")
    };
    assert!(
        text.contains("the title is kept; rename it in the window"),
        "{text}"
    );
    let OpResult::Thread(kept) = run(cx, Op::Thread(ThreadOp::Get { id: thread() })).unwrap()
    else {
        panic!("thread")
    };
    assert_eq!(kept.title, "Named");
}

#[gpui_kit::test]
fn approved_areas_dispatch_core_and_keep_database_transactions(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads", "database"], AREAS_PROBE);
    open(cx, &[installed(plugin.path())]);
    assert_eq!(
        run(
            cx,
            Op::Keymap(KeymapOp::Set {
                action: format!("{ID}:ping"),
                key: "ctrl-alt-o".into(),
                reset: false,
            })
        )
        .unwrap(),
        OpResult::Setting(json!({ "ok": true }))
    );
    let OpResult::ToolReply { text, is_error } = real_call(cx, "probe").unwrap() else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["thread"]["id"], THREAD);
    assert_eq!(value["thread"]["pinned"], true);
    assert_eq!(value["metadata"], json!({ "own": true }));
    assert_eq!(value["sidebar"]["threads"][0]["pinned"], true);
    assert!(value["sidebar"]["threads"][0].get("provider").is_none());
    assert_eq!(value["rows"], json!([{ "id": 1, "text": "kept" }]));

    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let commands = cx.update(|cx| crate::PluginHost::global(cx).read(cx).commands());
    assert_eq!(commands[0].key.as_deref(), Some("ctrl-alt-o"));
}

#[gpui_kit::test]
fn a_plugin_calls_every_thread_operation(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&["threads"], THREAD_OPS);
    let path = plugin.path().join(crate::manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["settings"]["worker"] = json!({ "type": "provider", "title": "Worker" });
    value["settings"]["spare"] = json!({ "type": "provider", "title": "Spare" });
    std::fs::write(&path, value.to_string()).unwrap();
    open(cx, &[installed(plugin.path())]);
    pick_worker(cx);
    let OpResult::ToolReply { text, is_error } = call_and_approve(cx).unwrap() else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let steps: Vec<Value> = serde_json::from_str(&text).unwrap();
    let step = |name: &str| {
        steps
            .iter()
            .find(|step| step["name"] == name)
            .unwrap_or_else(|| panic!("missing {name}"))
    };
    let ok = |name: &str| {
        let step = step(name);
        assert!(step["ok"].as_bool().unwrap(), "{name}: {}", step["error"]);
        step
    };
    let err = |name: &str, needle: &str| {
        let step = step(name);
        let error = step["error"].as_str().unwrap_or("");
        assert!(!step["ok"].as_bool().unwrap(), "{name}: {error}");
        assert!(error.contains(needle), "{name}: {error}");
    };
    assert!(ok("list")["value"].is_array());
    assert!(ok("listFiltered")["value"].is_array());
    let shown = &ok("get")["value"];
    assert_eq!(shown["id"], THREAD);
    hides_agent(shown);
    for listed in ok("list")["value"].as_array().unwrap() {
        hides_agent(listed);
    }
    assert!(ok("events")["value"].is_array());
    assert_eq!(ok("settle")["value"], "exited");
    err("send", "not found");
    err("output", "not found");
    let asked = &ok("ask")["value"];
    assert!(
        asked["interaction"]
            .as_str()
            .unwrap()
            .starts_with("interaction_")
    );
    err("wait", "unknown event type");
    ok("pin");
    ok("read");
    ok("writeMeta");
    assert_eq!(ok("meta")["value"], json!({ "n": 1 }));
    assert!(
        ok("projects")["value"]
            .as_array()
            .unwrap()
            .iter()
            .any(|project| project["id"] == "prj_test")
    );
    assert_eq!(ok("project")["value"]["id"], "prj_test");
    err("environment", "environment");
    err("restore", "no environment");
    err("spawn", "was not found");
    err("spawnMeta", "was not found");
    err("spawnAgent", "unknown field `agent`");
    err("spawnProvider", "unknown field `provider`");
    assert!(ok("settings")["value"].get("worker").is_none());
    ok("stop");
    ok("archive");
    ok("unarchive");
    ok("remove");
}

#[gpui_kit::test]
fn keymap_sets_removes_and_reads_plugin_keys_and_they_survive_reload(cx: &mut TestAppContext) {
    let plugin = probe_plugin(&[], COMMAND_PROBE);
    let path = plugin.path().join(crate::manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["commands"].as_array_mut().unwrap().extend([
        json!({ "id": "other", "title": "Other", "key": "alt-ctrl-o", "when": "always" }),
        json!({ "id": "third", "title": "Third", "key": "ctrl-alt-o", "when": "always" }),
    ]);
    std::fs::write(path, value.to_string()).unwrap();
    open(cx, &[installed(plugin.path())]);
    let set = |command: &str, key: &str| {
        Op::Keymap(KeymapOp::Set {
            action: format!("{ID}:{command}"),
            key: key.into(),
            reset: false,
        })
    };
    let remove = |command: &str| {
        Op::Keymap(KeymapOp::Remove {
            action: format!("{ID}:{command}"),
        })
    };
    assert!(matches!(
        run(cx, set("missing", "ctrl-alt-o")),
        Err(OpError::NotFound(_))
    ));
    assert!(run(cx, set("ping", "")).is_err());
    assert!(run(cx, set("ping", "ctrl-alt-junk")).is_err());
    assert!(run(cx, remove("missing")).is_err());
    assert_eq!(
        run(cx, set("ping", "ctrl-alt-o")).unwrap(),
        OpResult::Setting(json!({ "ok": false, "conflict": format!("{ID}:other") }))
    );
    run(cx, remove("other")).unwrap();
    run(cx, remove("third")).unwrap();
    assert_eq!(
        run(cx, set("ping", "ctrl-alt-o")).unwrap(),
        OpResult::Setting(json!({ "ok": true }))
    );
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let OpResult::Setting(disabled) = run(cx, Op::Keymap(KeymapOp::Get)).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(disabled["plugins"], json!({}));
    run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let OpResult::Setting(keys) = run(cx, Op::Keymap(KeymapOp::Get)).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(
        keys["plugins"],
        json!({
            format!("{ID}:ping"): "ctrl-alt-o",
            format!("{ID}:other"): null,
            format!("{ID}:third"): null,
        })
    );
    run(
        cx,
        Op::Keymap(KeymapOp::Set {
            action: format!("{ID}:other"),
            key: String::new(),
            reset: true,
        }),
    )
    .unwrap();
    let OpResult::Setting(keys) = run(cx, Op::Keymap(KeymapOp::Get)).unwrap() else {
        panic!("the keymap")
    };
    assert_eq!(keys["plugins"][format!("{ID}:other")], "ctrl-alt-o");
}

#[test]
fn database_refuses_external_files_and_transaction_control_in_statements() {
    use gpui_shell::HostArguments;
    let data = tempfile::tempdir().unwrap();
    let module = crate::database::module(&data.path().join("database.sqlite3"));
    let call = |name, args: Vec<Value>| {
        let args = HostArguments::new(args.into_iter().map(crate::area::to_host));
        futures::executor::block_on(module.begin(name, &args).unwrap())
            .map(|value| crate::area::from_host(&value))
    };
    let migration = json!(["CREATE TABLE notes (text TEXT NOT NULL)"]);
    call("migrate", vec![migration.clone()]).unwrap();
    call("migrate", vec![migration]).unwrap();
    assert!(call("migrate", vec![json!(["CREATE TABLE other (text TEXT)"])]).is_err());
    assert!(
        call(
            "run",
            vec![
                json!("ATTACH DATABASE ? AS other"),
                json!([data.path().join("other.sqlite3")])
            ]
        )
        .is_err()
    );
    assert!(!data.path().join("other.sqlite3").exists());
    assert!(call("run", vec![json!("PRAGMA foreign_keys = OFF"), json!([])]).is_err());
    assert!(
        call(
            "query",
            vec![
                json!("INSERT INTO notes VALUES ('bad') RETURNING text"),
                json!([])
            ]
        )
        .is_err()
    );
    assert!(
        call(
            "transaction",
            vec![json!([
                { "sql": "INSERT INTO notes VALUES ('rolled back')" },
                { "sql": "COMMIT" }
            ])]
        )
        .is_err()
    );
    assert_eq!(
        call("query", vec![json!("SELECT * FROM notes"), json!([])]).unwrap(),
        json!([])
    );
    call(
        "transaction",
        vec![json!([{ "sql": "INSERT INTO notes VALUES (?)", "params": ["kept"] }])],
    )
    .unwrap();
    assert_eq!(
        call("query", vec![json!("SELECT * FROM notes"), json!([])]).unwrap(),
        json!([{ "text": "kept" }])
    );
}

#[gpui_kit::test]
fn a_plugin_can_mkdir_under_process_data_dir(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    // mkdir runs off the test executor, so wait on real time instead of the
    // test clock, which would fire the tool timeout first.
    let task = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"data-dir"}),
            }),
            cx,
        )
    });
    let (sender, reply) = async_channel::bounded(1);
    cx.foreground_executor()
        .spawn(async move { sender.send(task.await).await })
        .detach();
    let result = (|| {
        for _ in 0..2000 {
            cx.run_until_parked();
            if let Ok(result) = reply.try_recv() {
                return result.unwrap();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("data-dir did not answer")
    })();
    let OpResult::ToolReply {
        text,
        is_error: false,
    } = result
    else {
        panic!("data-dir: {result:?}");
    };
    let expected = cx.read(|cx| {
        Workspace::global(cx)
            .read(cx)
            .data_dir()
            .join("plugins")
            .join(ID)
            .join("data")
    });
    assert_eq!(text, expected.display().to_string());
    assert!(expected.join("proof").is_dir(), "{text}");
}

#[gpui_kit::test]
fn a_plugin_with_no_powers_sees_none_of_the_environment(cx: &mut TestAppContext) {
    let named = ["HTTPS_PROXY", "PATH", "HOME", "USER", "HOSTNAME", "SHELL"];
    // SAFETY: nextest runs each test in its own process, and nothing else here reads the
    // environment while it is set.
    unsafe {
        std::env::set_var("HTTPS_PROXY", "http://user:secret@proxy.test:8080");
        for name in &named[1..] {
            if std::env::var_os(name).is_none() {
                std::env::set_var(name, "set");
            }
        }
    }
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["powers"] = json!({});
    write_plugin(plugin.path(), &declared);
    open(cx, &[installed(plugin.path())]);
    let asked = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"env"}),
            }),
            cx,
        )
    });
    let OpResult::ToolReply { text, .. } = cx.foreground_executor().block_test(asked).unwrap()
    else {
        panic!("env");
    };
    let seen: Vec<String> = serde_json::from_str(&text).unwrap();
    for name in named {
        assert!(
            !seen.iter().any(|seen| seen.eq_ignore_ascii_case(name)),
            "{name} in {seen:?}"
        );
    }
}

#[gpui_kit::test]
fn context_waiters_receive_changes_and_are_cancelled_on_unload(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, EmptyView, WindowOptions};
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let watch = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"watch"}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    let window = cx.update(|cx| {
        let window = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, cx| {
            ui.set_context(
                window.into(),
                Some(thread()),
                Some(ProjectId("proj_1".into())),
                true,
                &[],
                cx,
            );
        });
        window.window_id()
    });
    let OpResult::ToolReply { text, .. } = cx.foreground_executor().block_test(watch).unwrap()
    else {
        panic!("context");
    };
    let context: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(context["threadId"], THREAD);
    assert_eq!(context["focused"], true);
    assert!(context.get("overlay").is_none());
    let watch = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"watch"}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, _| {
            ui.set_overlay(
                window,
                gpui_kit::size(gpui_kit::px(640.), gpui_kit::px(480.)),
            );
        });
    });
    let OpResult::ToolReply { text, .. } = cx.foreground_executor().block_test(watch).unwrap()
    else {
        panic!("overlay");
    };
    let context: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(context["overlay"], json!({ "width": 640, "height": 480 }));
    let watch = cx.update(|cx| {
        Workspace::dispatch(
            Op::Thread(ThreadOp::CallTool {
                id: thread(),
                name: "echo".into(),
                input: json!({"text":"watch"}),
            }),
            cx,
        )
    });
    cx.run_until_parked();
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    assert!(cx.foreground_executor().block_test(watch).is_err());
}

#[gpui_kit::test]
fn the_overlay_size_is_the_front_windows(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, EmptyView, WindowOptions};
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let size = cx.update(|cx| {
        let back = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let front = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, cx| {
            ui.set_context(back.into(), None, None, false, &[], cx);
            ui.set_context(front.into(), None, None, true, &[], cx);
            ui.set_overlay(
                back.window_id(),
                gpui_kit::size(gpui_kit::px(300.), gpui_kit::px(200.)),
            );
            ui.set_overlay(
                front.window_id(),
                gpui_kit::size(gpui_kit::px(640.), gpui_kit::px(480.)),
            );
            crate::area::from_host(&ui.context_for(&PluginId(ID.into())))["overlay"].clone()
        })
    });
    assert_eq!(size, json!({ "width": 640, "height": 480 }));
}

#[gpui_kit::test]
fn a_right_panel_loads_without_a_skip_note(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"]["panels"] = json!([
        {"id":"side","title":"Side","view":"Side","placement":"right"},
        {"id":"tool","title":"Tool","view":"Side","placement":"left"}
    ]);
    write_plugin(plugin.path(), &declared);
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            "{MAIN}\nexport class Side extends View {{\n  render() {{ return \"side\"; }}\n}}\n"
        ),
    )
    .unwrap();
    open(cx, &[installed(plugin.path())]);
    let read = |cx: &mut TestAppContext| {
        cx.read(|cx| {
            let host = crate::PluginHost::global(cx).read(cx);
            let status = host
                .status()
                .into_iter()
                .find(|status| status.id.0 == ID)
                .expect("status");
            (
                status.error.clone(),
                status.note.as_deref().map(str::to_owned),
                host.failure(&PluginId(ID.into())),
            )
        })
    };
    let (error, note, failure) = read(cx);
    assert!(error.is_none(), "{error:?}");
    assert!(failure.is_none(), "{failure:?}");
    assert!(note.is_none(), "{note:?}");
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                crate::PluginHost::global(cx)
                    .update(cx, |host, cx| {
                        host.mount_panel(&PluginId(ID.into()), "side", window, cx)
                    })
                    .unwrap()
            })
            .unwrap();
    });
    let (error, note, failure) = read(cx);
    assert!(error.is_none(), "{error:?}");
    assert!(failure.is_none(), "{failure:?}");
    assert!(note.is_none(), "{note:?}");
}

#[gpui_kit::test]
fn a_named_export_mounts_beside_the_default_view(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"]["panels"] = json!([
        {"id":"side","title":"Side","view":"Side","placement":"right"}
    ]);
    write_plugin(plugin.path(), &declared);
    open(cx, &[installed(plugin.path())]);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                let host = crate::PluginHost::global(cx);
                let missing = host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId(ID.into()), "missing", window, cx)
                });
                assert!(
                    matches!(missing, Err(crate::MountError::Missing(ref message)) if message.contains("has no panel")),
                    "{missing:?}"
                );
                let failed = host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId(ID.into()), "side", window, cx)
                });
                let Err(crate::MountError::Failed(error)) = failed else {
                    panic!("{failed:?}");
                };
                assert!(error.contains("export `Side`"), "{error}");
                assert!(!host.update(cx, |host, cx| {
                    host.reject_mount(&PluginId(ID.into()), &error, cx)
                }));
            })
            .unwrap();
    });
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );

    let plugin = plugin_dir();
    write_plugin(plugin.path(), &declared);
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            "{MAIN}\nexport class Side extends View {{\n  render() {{ return \"side\"; }}\n}}\n"
        ),
    )
    .unwrap();
    open(cx, &[installed(plugin.path())]);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    let side = cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                let host = crate::PluginHost::global(cx);
                host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId(ID.into()), "side", window, cx)
                })
                .unwrap()
            })
            .unwrap()
    });
    assert_eq!(
        call(cx, "echo", json!({ "text": "hi" })).unwrap(),
        OpResult::ToolReply {
            text: format!("{ID} hi true"),
            is_error: false
        }
    );
    drop(side);
}

#[gpui_kit::test]
fn a_named_mount_that_fails_to_construct_unloads_and_logs(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let mut declared = sample_manifest();
    declared["ui"]["panels"] = json!([
        {"id":"side","title":"Side","view":"Side","placement":"right"}
    ]);
    write_plugin(plugin.path(), &declared);
    std::fs::write(
        plugin.path().join("main.js"),
        format!(
            "{MAIN}\nexport class Side extends View {{\n  init() {{ throw new Error(\"side broke\"); }}\n  render() {{ return \"side\"; }}\n}}\n"
        ),
    )
    .unwrap();
    open(cx, &[installed(plugin.path())]);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    let failed = cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                crate::PluginHost::global(cx).update(cx, |host, cx| {
                    host.mount_named_view(&PluginId(ID.into()), "Side", window, cx)
                })
            })
            .unwrap()
    });
    let Err(crate::MountError::Failed(error)) = failed else {
        panic!("{failed:?}");
    };
    assert!(error.contains("side broke"), "{error}");
    assert!(!loaded(cx));
    cx.run_until_parked();
    let log = cx.read(|cx| {
        let host = cx.global::<PluginHostGlobal>().0.read(cx);
        let path = host.data_dir.join("plugins").join(ID).join("logs.jsonl");
        std::fs::read_to_string(path).unwrap_or_default()
    });
    assert!(log.contains("side broke"), "{log}");
    assert!(log.contains("\"level\":\"error\""), "{log}");
}

#[gpui_kit::test]
fn open_panel_props_stay_on_the_plugin_that_sent_them(cx: &mut TestAppContext) {
    use gpui_kit::{AppContext as _, EmptyView, WindowOptions};
    open(cx, &[]);
    let mine = PluginId(ID.into());
    let other = PluginId("dev.test.other".into());
    let (sender, receiver) = async_channel::bounded(1);
    cx.update(|cx| {
        let window = cx
            .open_window(WindowOptions::default(), |_, cx| cx.new(|_| EmptyView))
            .unwrap();
        let ui = crate::HostUi::global(cx).unwrap();
        ui.update(cx, |ui, cx| {
            let window: gpui_kit::AnyWindowHandle = window.into();
            ui.set_context(window, Some(thread()), None, true, &[], cx);
            ui.push_waiter(other.clone(), sender);
            ui.deliver(window.window_id(), &mine, json!({"panel": "side"}));
        });
        let ui = crate::HostUi::global(cx).unwrap();
        let mine_context = {
            let value = ui.read(cx).context_for(&mine);
            crate::area::from_host(&value)
        };
        let other_context = {
            let value = ui.read(cx).context_for(&other);
            crate::area::from_host(&value)
        };
        assert_eq!(mine_context["props"], json!({"panel": "side"}));
        assert!(other_context.get("props").is_none(), "{other_context}");
        assert_eq!(other_context["focused"], true);
    });
    let waited = crate::area::from_host(&receiver.try_recv().unwrap());
    assert!(
        waited.get("props").is_none(),
        "plugin B must not see A's props: {waited}"
    );
    assert_eq!(waited["threadId"], THREAD);
}

#[gpui_kit::test]
fn process_run_answers_inside_a_script_call(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, args) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let output = forward(cx, "process", "run", &json!([name, args])).unwrap();
    assert_eq!(output["exitCode"], 0);
    assert_eq!(output["stdout"].as_str().unwrap().trim(), "hello");
    assert_eq!(output["stderr"].as_str().unwrap().trim(), "error");
    let refused = forward(cx, "process", "run", &json!(["unapproved", []])).unwrap_err();
    assert!(refused.contains("not granted"), "{refused}");
}

#[gpui_kit::test]
fn threads_output_reads_scrollback_plain_text(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let plugin = probe_plugin(&["threads"], THREADS_FORWARD);
    let mut value = sample_manifest();
    value["powers"] = json!({ "areas": ["threads"] });
    value["tools"].as_array_mut().unwrap().push(json!({
        "name": "probe",
        "description": "Forward a host call",
        "input": "schemas/any.json",
        "scope": "all"
    }));
    write_plugin(plugin.path(), &value);
    std::fs::write(plugin.path().join("main.js"), THREADS_FORWARD).unwrap();
    open(cx, &[installed(plugin.path())]);

    let program = if cfg!(windows) {
        "cmd.exe".into()
    } else {
        "/bin/sh".into()
    };
    let OpResult::Thread(started) = run(
        cx,
        Op::Thread(ThreadOp::Run(Run {
            project_id: ProjectId("prj_test".into()),
            program,
            title: Some("Output".into()),
            environment: EnvironmentSpec::ProjectDefault,
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        })),
    )
    .unwrap() else {
        panic!("run")
    };
    for _ in 0..200 {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(5));
        let status = cx.update(|cx| {
            Workspace::global(cx)
                .read(cx)
                .thread(&started.id)
                .map(|thread| thread.read(cx).record().status)
        });
        if matches!(status, Some(ThreadStatus::Active | ThreadStatus::Idle)) {
            break;
        }
    }
    let marker = "kagan-output-marker";
    let typed = if cfg!(windows) {
        format!("echo {marker}\r\n")
    } else {
        format!("printf '{marker}\\n'\n")
    };
    run(
        cx,
        Op::Thread(ThreadOp::Input {
            id: started.id.clone(),
            text: typed,
            terminal: None,
        }),
    )
    .unwrap();
    let mut text = String::new();
    for _ in 0..200 {
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(5));
        let value = forward(cx, "threads", "output", &json!([started.id.0])).unwrap();
        text = value["output"].as_str().unwrap_or("").to_owned();
        if text.contains(marker) {
            break;
        }
    }
    assert!(text.contains(marker), "output missing {marker}: {text:?}");
    let missing = forward(cx, "threads", "output", &json!(["thr_missing"])).unwrap_err();
    assert!(missing.contains("not found"), "{missing}");
}

#[gpui_kit::test]
fn process_run_times_out(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let receive = dispatched(
        cx,
        forwarded(
            "process",
            "run",
            &json!([name, sleeping(), { "timeoutMs": 1 }]),
        ),
    );
    let error = forwarded_reply(settled(cx, &receive, Duration::from_millis(1))).unwrap_err();
    assert!(error.contains("timed out"), "{error}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn unloading_a_plugin_ends_its_running_command_and_kills_the_process_tree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let (pids, receive) = start_tree(cx, name);
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let error = settled(cx, &receive, Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("unloaded"), "{error}");
    for _ in 0..2000 {
        if pids.iter().all(|pid| !alive(pid)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the process tree outlived the plugin: {pids:?}");
}

#[cfg(unix)]
#[gpui_kit::test]
fn quitting_the_app_ends_a_running_command_and_kills_the_process_tree(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let (pids, _receive) = start_tree(cx, name);
    cx.quit();
    for _ in 0..2000 {
        if pids.iter().all(|pid| !alive(pid)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("the process tree outlived the app: {pids:?}");
}

#[gpui_kit::test]
fn unloading_a_plugin_ends_a_running_command_with_an_unloaded_error(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (name, _) = shell();
    let _plugin = forwarding(cx, json!({ "run": [{ "program": name }] }));
    let receive = dispatched(cx, forwarded("process", "run", &json!([name, sleeping()])));
    cx.run_until_parked();
    run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    )
    .unwrap();
    let error = settled(cx, &receive, Duration::ZERO).unwrap_err();
    assert!(error.to_string().contains("unloaded"), "{error}");
}

#[gpui_kit::test]
fn bundled_examples_run_through_the_host(cx: &mut TestAppContext) {
    let examples = crate::examples();
    let plugins: Vec<_> = examples
        .iter()
        .map(|example| installed_example(&example.root))
        .collect();
    assert!(plugins.len() >= 2, "{:?}", examples.len());
    open(cx, &plugins);
    let handle = cx.add_window(|_, _| gpui_kit::EmptyView);
    let mounted = cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                let host = crate::PluginHost::global(cx);
                host.update(cx, |host, cx| {
                    host.mount_panel(&PluginId("dev.kagan.panel".into()), "hello", window, cx)
                })
                .unwrap()
            })
            .unwrap()
    });
    assert_eq!(
        answered(
            cx,
            Op::Plugin(PluginOp::Cli {
                name: "hello".into(),
                command: "hi".into(),
                argv: Vec::new(),
                cwd: "/work".into(),
                thread_id: None,
                project_id: None,
            }),
        )
        .unwrap(),
        OpResult::Cli {
            exit_code: 0,
            stdout: "Hello\n".into(),
            stderr: String::new(),
        }
    );
    drop(mounted);
}

#[gpui_kit::test]
fn a_disabled_plugin_keeps_its_name_and_version_from_its_manifest(cx: &mut TestAppContext) {
    // A plugin that is off is never activated. Its manifest still names it, as at a restart.
    let plugin = plugin_dir();
    let mut record = installed(plugin.path());
    record.enabled = false;
    open(cx, &[record]);
    let status = cx.read(|cx| {
        crate::PluginHost::global(cx)
            .read(cx)
            .status()
            .into_iter()
            .find(|status| status.id.0 == ID)
            .expect("the plugin is listed")
    });
    assert!(!status.enabled);
    assert_eq!(status.name.as_str(), "Echo");
    assert_eq!(status.version.as_deref(), Some("1.0.0"));
}

#[gpui_kit::test]
fn a_project_setting_takes_a_live_project_id(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    open(cx, &[installed(plugin.path())]);
    let config = |op| {
        Op::Plugin(PluginOp::Config {
            id: PluginId(ID.into()),
            op,
            thread_id: None,
        })
    };
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(Value::Null)
    );
    let gone = run(
        cx,
        config(ConfigOp::Set {
            key: "board".into(),
            value: Some(json!("prj_gone")),
        }),
    )
    .unwrap_err();
    assert!(gone.to_string().contains("no project prj_gone"), "{gone}");
    run(
        cx,
        config(ConfigOp::Set {
            key: "board".into(),
            value: Some(json!("prj_test")),
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(json!("prj_test"))
    );
    run(
        cx,
        config(ConfigOp::Clear {
            key: "board".into(),
        }),
    )
    .unwrap();
    assert_eq!(
        run(
            cx,
            config(ConfigOp::Get {
                key: "board".into()
            })
        )
        .unwrap(),
        OpResult::Setting(Value::Null)
    );
}
