use std::path::Path;
use std::time::Duration;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{ConfigOp, Op, OpResult, PluginId, PluginOp, ThreadOp};
use serde_json::{Value, json};

use super::{
    ID, Sweep, Unlink, answer, files_plugin, home, installed, open, open_kept, plugin_dir,
    plugin_op, point_at, refused_read, run, sample_manifest, slice, thread, unique, write_plugin,
};

#[gpui_kit::test]
fn a_folder_that_becomes_text_loses_its_read_root(cx: &mut TestAppContext) {
    let plugin = plugin_dir();
    let folders = ["approved", "typed", "default"].map(|name| {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("secret.txt"), name).unwrap();
        dir
    });
    let path = |ix: usize| {
        dunce::canonicalize(folders[ix].path())
            .unwrap()
            .display()
            .to_string()
    };
    // The file read runs off the test executor, so wait on real time instead of the
    // test clock, which would fire the tool timeout first.
    let peek = |cx: &mut TestAppContext, folder: &str| {
        let input = json!({ "path": Path::new(folder).join("secret.txt") });
        let task = cx.update(|cx| {
            Workspace::dispatch(
                Op::Thread(ThreadOp::CallTool {
                    id: thread(),
                    name: "peek".into(),
                    input,
                }),
                cx,
            )
        });
        let (sender, reply) = async_channel::bounded(1);
        cx.foreground_executor()
            .spawn(async move { sender.send(task.await).await })
            .detach();
        for _ in 0..2000 {
            cx.run_until_parked();
            if let Ok(result) = reply.try_recv() {
                return result.unwrap();
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("peek did not answer")
    };
    let config = |value: String, thread_id| PluginOp::Config {
        id: PluginId(ID.into()),
        op: ConfigOp::Set {
            key: "notes".into(),
            value: Some(json!(value)),
        },
        thread_id,
    };
    open(cx, &[installed(plugin.path())]);

    let setting = plugin_op(cx, config(path(0), Some(thread())));
    answer(cx, "approve");
    cx.foreground_executor().block_test(setting).unwrap();
    assert_eq!(
        peek(cx, &path(0)),
        OpResult::ToolReply {
            text: "approved".into(),
            is_error: false
        }
    );

    let mut text = sample_manifest();
    text["settings"]["notes"] = json!({ "type": "string", "title": "Notes", "default": path(2) });
    write_plugin(plugin.path(), &text);
    let id = || PluginId(ID.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: id() })).unwrap();
    run(cx, Op::Plugin(PluginOp::Enable { id: id() })).unwrap();
    run(cx, Op::Plugin(config(path(1), None))).unwrap();
    for folder in [path(0), path(1), path(2)] {
        let OpResult::ToolReply { is_error, .. } = peek(cx, &folder) else {
            panic!("tool reply")
        };
        assert!(is_error, "{folder} must not be readable");
    }
}

#[gpui_kit::test]
fn a_declared_folder_is_readable_in_ranges_and_a_sibling_is_not(cx: &mut TestAppContext) {
    let name = unique("read");
    let root = home().join(&name);
    let allowed = root.join("allowed");
    let sibling = root.join("sibling");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&sibling).unwrap();
    let _sweep = Sweep(root);
    std::fs::write(allowed.join("note.txt"), "hello-allowed").unwrap();
    std::fs::write(sibling.join("note.txt"), "secret-sibling").unwrap();
    let big = allowed.join("big.txt");
    std::fs::write(
        &big,
        "A".repeat(1_000) + &"B".repeat(1_000) + &"C".repeat(1_000),
    )
    .unwrap();

    let plugin = plugin_dir();
    files_plugin(plugin.path(), &format!("~/{name}/allowed"));
    open(cx, &[installed(plugin.path())]);

    let OpResult::ToolReply { text, is_error } =
        slice(cx, json!({ "path": big, "offset": 0, "maxBytes": 1000 }))
    else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["text"], "A".repeat(1_000));
    assert_eq!(value["next"], 1000);
    assert_eq!(value["size"], 3000);

    let OpResult::ToolReply { text, is_error } =
        slice(cx, json!({ "path": big, "offset": 1000, "maxBytes": 1000 }))
    else {
        panic!("tool reply")
    };
    assert!(!is_error, "{text}");
    let value: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(value["text"], "B".repeat(1_000));
    assert_eq!(value["next"], 2000);

    let OpResult::ToolReply { text, is_error } =
        slice(cx, json!({ "list": true, "path": &sibling }))
    else {
        panic!("tool reply")
    };
    assert!(is_error, "{text}");
    assert!(text.contains("not a granted folder"), "{text}");
}

#[gpui_kit::test]
fn declaring_the_data_dir_a_folder_inside_it_or_ssh_is_refused(cx: &mut TestAppContext) {
    let data = tempfile::Builder::new()
        .prefix(".ms89-")
        .tempdir_in(home())
        .unwrap();
    let name = data
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    std::fs::create_dir_all(data.path().join("notes")).unwrap();
    let shown = dunce::canonicalize(data.path()).unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let data_entry = format!("~/{name}");
    let error = refused_read(cx, &data_entry);
    assert!(
        error.contains(&format!(
            "read path \"{data_entry}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );

    let inside = format!("~/{name}/notes");
    let error = refused_read(cx, &inside);
    assert!(
        error.contains(&format!(
            "read path \"{inside}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );

    let error = refused_read(cx, "~/.ssh");
    assert!(
        error.contains("read path \"~/.ssh\" cannot cover ~/.ssh"),
        "{error}"
    );
}

#[gpui_kit::test]
fn a_declared_folder_that_contains_a_data_dir_outside_home_is_refused(cx: &mut TestAppContext) {
    let parent = tempfile::tempdir().unwrap();
    let data = parent.path().join("data");
    std::fs::create_dir(&data).unwrap();
    let name = unique("cover");
    let link = home().join(&name);
    let _unlink = Unlink(link.clone());
    point_at(&link, parent.path());
    let shown = dunce::canonicalize(&data).unwrap();
    open_kept(cx, &data, parent, &[]);
    let entry = format!("~/{name}");
    let error = refused_read(cx, &entry);
    assert!(
        error.contains(&format!(
            "read path \"{entry}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );
}

#[gpui_kit::test]
fn check_refuses_a_read_of_the_data_dir(cx: &mut TestAppContext) {
    let data = tempfile::Builder::new()
        .prefix(".ms89-")
        .tempdir_in(home())
        .unwrap();
    let name = data
        .path()
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let shown = dunce::canonicalize(data.path()).unwrap();
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let plugin = plugin_dir();
    let clean = plugin_op(
        cx,
        PluginOp::Check {
            path: plugin.path().to_path_buf(),
        },
    );
    assert_eq!(
        cx.foreground_executor().block_test(clean).unwrap(),
        OpResult::Done
    );

    let entry = format!("~/{name}");
    let mut value = sample_manifest();
    value["powers"]["read"] = json!([entry]);
    write_plugin(plugin.path(), &value);
    let checking = plugin_op(
        cx,
        PluginOp::Check {
            path: plugin.path().to_path_buf(),
        },
    );
    let error = cx
        .foreground_executor()
        .block_test(checking)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains(&format!(
            "read path \"{entry}\" cannot cover {}",
            shown.display()
        )),
        "{error}"
    );
}
