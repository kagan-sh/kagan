use std::path::Path;
use std::sync::Arc;

use gpui_kit::TestAppContext;
use kg_core::Workspace;
use kg_domain::{ConfigOp, DiagnosticsOp, Op, OpError, OpResult, PluginId, PluginOp, ThreadId};
use serde_json::{Value, json};
use tempfile::TempDir;

use super::{ID, answer_standalone, call, installed, open, plugin_op, probe_plugin, run, thread};
use crate::secret::Store;
use keyring_core::CredentialStore;

const SECRET: &str = "lin_api_7f3c9d1e";

const READS_SECRET: &str = r#"
import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
import { settings, log, toast } from "kagan/host";
const reversed = (text) => text && [...text].reverse().join("");
export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        const token = settings().token;
        log("info", `calling Linear with ${token}`);
        if (job.name === "broken") {
          fail(job.id, `401 for Bearer ${token}`);
        } else if (job.name === "peek") {
          toast(`Linear failed: ${token}`);
          reply(job.id, { text: "toasted" });
        } else {
          reply(job.id, { text: JSON.stringify([token, reversed(token)]) });
        }
      }
    });
  }
  render() { return "secret"; }
}
export class Side extends View {
  init() { throw new Error(`401 for Bearer ${settings().token}`); }
  render() { return "side"; }
}
"#;

fn secret_plugin() -> TempDir {
    let plugin = probe_plugin(&[], READS_SECRET);
    let path = plugin.path().join(crate::manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["settings"]["token"] = json!({ "type": "secret", "title": "Linear API key" });
    value["ui"]["panels"] =
        json!([{ "id": "side", "title": "Side", "view": "Side", "placement": "left" }]);
    std::fs::write(&path, value.to_string()).unwrap();
    plugin
}

fn config(op: ConfigOp, thread_id: Option<ThreadId>) -> Op {
    Op::Plugin(PluginOp::Config {
        id: PluginId(ID.into()),
        op,
        thread_id,
    })
}

fn token(cx: &mut TestAppContext, thread_id: Option<ThreadId>) -> Value {
    let get = ConfigOp::Get {
        key: "token".into(),
    };
    let Ok(OpResult::Setting(value)) = run(cx, config(get, thread_id)) else {
        panic!("setting")
    };
    value
}

fn set(cx: &mut TestAppContext, value: Value) -> Result<OpResult, OpError> {
    let set = ConfigOp::Set {
        key: "token".into(),
        value: Some(value),
    };
    run(cx, config(set, None))
}

/// What the plugin read, as its tool reply shows it, and the text it reversed: a reply holds
/// `[secret]` where the text was, so only the reversed copy proves the plugin had it.
fn plugin_reads(cx: &mut TestAppContext) -> (Value, Option<String>) {
    let Ok(OpResult::ToolReply { text, .. }) = call(cx, "probe", json!({})) else {
        panic!("tool reply")
    };
    let [shown, reversed]: [Value; 2] = serde_json::from_str(&text).unwrap();
    let read = reversed.as_str().map(|text| text.chars().rev().collect());
    (shown, read)
}

fn data_dir(cx: &mut TestAppContext) -> std::path::PathBuf {
    cx.read(|cx| Workspace::global(cx).read(cx).data_dir().to_owned())
}

fn holding(dir: &Path, needle: &str, skip: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path == skip {
            continue;
        }
        if path.is_dir() {
            found.extend(holding(&path, needle, skip));
        } else if std::fs::read(&path)
            .unwrap()
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
        {
            found.push(path);
        }
    }
    found
}

#[gpui_kit::test]
fn a_secret_reaches_its_plugin_and_no_one_else(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    assert_eq!(token(cx, None), Value::Null);
    assert_eq!(plugin_reads(cx), (Value::Null, None));

    let OpResult::Warnings(warnings) = set(cx, json!(SECRET)).unwrap() else {
        panic!("a warning: tests have no keychain, so the secret goes to a file")
    };
    assert!(
        warnings[0].contains("files only you can read"),
        "{warnings:?}"
    );
    assert_eq!(token(cx, None), json!("set"));
    assert_eq!(token(cx, Some(thread())), json!("set"));
    assert_eq!(plugin_reads(cx), (json!("[secret]"), Some(SECRET.into())));

    let Ok(OpResult::Logs { text, .. }) = run(
        cx,
        Op::Plugin(PluginOp::Logs {
            id: PluginId(ID.into()),
            follow: false,
            after: None,
        }),
    ) else {
        panic!("logs")
    };
    assert!(text.contains("calling Linear with [secret]"), "{text}");
    let Ok(OpResult::Diagnostics { path }) =
        run(cx, Op::Diagnostics(DiagnosticsOp::Export { path: None }))
    else {
        panic!("diagnostics")
    };
    assert!(path.exists());
    let data = data_dir(cx);
    let secrets = data.join("secrets");
    assert_eq!(
        holding(&data, SECRET, &secrets),
        Vec::<std::path::PathBuf>::new()
    );
    let file = secrets.join(format!("{ID}.json"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&file), 0o600);
        assert_eq!(mode(&secrets), 0o700);
    }

    let replace = run(
        cx,
        config(
            ConfigOp::Set {
                key: "token".into(),
                value: None,
            },
            None,
        ),
    );
    assert!(replace.unwrap_err().to_string().contains("prompt or stdin"));
    let refused = set(cx, json!("two\nlines")).unwrap_err().to_string();
    assert!(
        refused.contains("one line of text") && !refused.contains("two"),
        "{refused}"
    );
    assert_eq!(plugin_reads(cx).1.as_deref(), Some(SECRET));

    let clear = ConfigOp::Clear {
        key: "token".into(),
    };
    assert_eq!(run(cx, config(clear, None)).unwrap(), OpResult::Done);
    assert_eq!(token(cx, None), Value::Null);
    assert_eq!(plugin_reads(cx), (Value::Null, None));
    assert!(!file.exists());
    let plain = ConfigOp::Clear {
        key: "sounds".into(),
    };
    let error = run(cx, config(plain, None)).unwrap_err().to_string();
    assert!(error.contains("not a secret"), "{error}");
}

#[gpui_kit::test]
fn a_secret_is_read_again_on_enable_and_goes_with_its_plugin(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    set(cx, json!(SECRET)).unwrap();
    let disable = run(
        cx,
        Op::Plugin(PluginOp::Disable {
            id: PluginId(ID.into()),
        }),
    );
    disable.unwrap();
    let enable = run(
        cx,
        Op::Plugin(PluginOp::Enable {
            id: PluginId(ID.into()),
        }),
    );
    enable.unwrap();
    assert_eq!(token(cx, None), json!("set"));
    assert_eq!(plugin_reads(cx).1.as_deref(), Some(SECRET));

    let file = data_dir(cx).join("secrets").join(format!("{ID}.json"));
    assert!(file.exists());
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id: PluginId(ID.into()),
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.run_until_parked();
    cx.foreground_executor().block_test(removing).unwrap();
    assert!(!file.exists());
}

fn plugin_text(cx: &mut TestAppContext, op: PluginOp) -> String {
    serde_json::to_string(&run(cx, Op::Plugin(op)).unwrap()).unwrap()
}

#[gpui_kit::test]
fn an_error_holding_a_secret_shows_it_hidden_to_every_caller(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    set(cx, json!(SECRET)).unwrap();

    let Ok(OpResult::ToolReply { text, is_error }) = call(cx, "broken", json!({})) else {
        panic!("tool reply")
    };
    assert!(is_error);
    assert_eq!(text, "401 for Bearer [secret]");

    let window = cx.add_window(|_, _| gpui_kit::EmptyView);
    let mounted = cx.update(|cx| {
        window
            .update(cx, |_, window, cx| {
                crate::PluginHost::global(cx).update(cx, |host, cx| {
                    host.mount_named_view(&PluginId(ID.into()), "Side", window, cx)
                })
            })
            .unwrap()
    });
    let Err(crate::MountError::Failed(error)) = mounted else {
        panic!("{mounted:?}")
    };
    assert!(error.contains("401 for Bearer [secret]"), "{error}");
    cx.run_until_parked();

    let listed = plugin_text(cx, PluginOp::List);
    assert!(listed.contains("401 for Bearer [secret]"), "{listed}");
    let logs = plugin_text(
        cx,
        PluginOp::Logs {
            id: PluginId(ID.into()),
            follow: false,
            after: None,
        },
    );
    assert!(logs.contains("401 for Bearer [secret]"), "{logs}");
    for shown in [error, listed, logs] {
        assert!(!shown.contains(SECRET), "{shown}");
    }
}

fn use_mock_keychain(cx: &mut TestAppContext) -> Arc<CredentialStore> {
    let mock: Arc<CredentialStore> = keyring_core::mock::Store::new().unwrap();
    let keychain = mock.clone();
    cx.update(|cx| {
        crate::PluginHost::global(cx).update(cx, |host, _| {
            host.secrets = Store::new(Some(keychain), host.data_dir.clone());
        });
    });
    mock
}

fn keychain_item(mock: &Arc<CredentialStore>, data: &Path, key: &str) -> keyring_core::Entry {
    let account = format!("{ID}/{key} in {}", data.display());
    mock.build("kagan", &account, None).unwrap()
}

fn disable_and_enable(cx: &mut TestAppContext) {
    let id = PluginId(ID.into());
    run(cx, Op::Plugin(PluginOp::Disable { id: id.clone() })).unwrap();
    run(cx, Op::Plugin(PluginOp::Enable { id })).unwrap();
}

fn remove(cx: &mut TestAppContext) -> OpResult {
    let id = PluginId(ID.into());
    let removing = plugin_op(
        cx,
        PluginOp::Remove {
            id,
            thread_id: None,
        },
    );
    answer_standalone(cx, "approve");
    cx.run_until_parked();
    cx.foreground_executor().block_test(removing).unwrap()
}

#[gpui_kit::test]
fn a_keychain_secret_goes_with_its_plugin_even_when_its_folder_is_gone(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    let mock = use_mock_keychain(cx);
    assert_eq!(
        set(cx, json!(SECRET)).unwrap(),
        OpResult::Done,
        "no warning"
    );
    let data = data_dir(cx);
    let item = keychain_item(&mock, &data, "token");
    assert_eq!(item.get_password().unwrap(), SECRET);
    let kept = std::fs::read_to_string(data.join("secrets").join(format!("{ID}.json"))).unwrap();
    assert!(kept.contains("token") && !kept.contains(SECRET), "{kept}");
    disable_and_enable(cx);
    assert_eq!(plugin_reads(cx).1.as_deref(), Some(SECRET));

    std::fs::remove_dir_all(plugin.path()).unwrap();
    assert_eq!(remove(cx), OpResult::Done);
    assert!(matches!(
        item.get_password(),
        Err(keyring_core::Error::NoEntry)
    ));
}

#[gpui_kit::test]
fn an_update_that_drops_a_secret_forgets_it(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    let mock = use_mock_keychain(cx);
    set(cx, json!(SECRET)).unwrap();
    let path = plugin.path().join(crate::manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["settings"].as_object_mut().unwrap().remove("token");
    std::fs::write(&path, value.to_string()).unwrap();
    disable_and_enable(cx);
    let item = keychain_item(&mock, &data_dir(cx), "token");
    assert!(matches!(
        item.get_password(),
        Err(keyring_core::Error::NoEntry)
    ));
}

#[gpui_kit::test]
fn a_keychain_that_refuses_does_not_block_a_removal(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    let mock = use_mock_keychain(cx);
    set(cx, json!(SECRET)).unwrap();
    let item = keychain_item(&mock, &data_dir(cx), "token");
    let cred: &keyring_core::mock::Cred = item.as_any().downcast_ref().unwrap();
    cred.set_error(keyring_core::Error::PlatformFailure("locked".into()));

    let OpResult::Warnings(warnings) = remove(cx) else {
        panic!("a warning names the secret left behind")
    };
    assert!(warnings[0].contains("token"), "{warnings:?}");
    assert!(super::records(cx).is_empty());
}

#[test]
fn saves_at_the_same_time_keep_every_secret() {
    let data = tempfile::tempdir().unwrap();
    let store = Store::new(None, data.path().to_owned());
    let id = PluginId(ID.into());
    let keys: Vec<String> = (0..32).map(|n| format!("key{n}")).collect();
    let start = std::sync::Barrier::new(keys.len());
    std::thread::scope(|scope| {
        for key in &keys {
            let (store, id, start) = (&store, &id, &start);
            scope.spawn(move || {
                start.wait();
                store.set(id, key, "text").unwrap();
            });
        }
    });
    let fresh = Store::new(None, data.path().to_owned());
    assert!(fresh.load(&id, &keys).is_empty());
    assert_eq!(fresh.texts(&id).len(), keys.len());
}

#[test]
fn a_secret_that_starts_another_is_hidden_whole() {
    let data = tempfile::tempdir().unwrap();
    let store = Store::new(None, data.path().to_owned());
    let id = PluginId(ID.into());
    store.set(&id, "a", "lin_api").unwrap();
    store.set(&id, "b", "lin_api_long").unwrap();
    assert_eq!(store.redact(&id, "got lin_api_long"), "got [secret]");
}

#[test]
fn a_secret_key_fits_the_keychain() {
    let plugin = secret_plugin();
    let path = plugin.path().join(crate::manifest::FILE);
    let mut value: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    value["settings"]["k".repeat(65)] = json!({ "type": "secret", "title": "Long" });
    std::fs::write(&path, value.to_string()).unwrap();
    let error = super::read_manifest(plugin.path()).unwrap_err();
    assert!(error.to_string().contains("at most 64 bytes"), "{error}");
}

#[gpui_kit::test]
fn a_toast_holding_a_secret_shows_it_hidden(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    set(cx, json!(SECRET)).unwrap();
    let (sender, toasts) = async_channel::unbounded();
    cx.update(|cx| {
        let ui = crate::HostUi::global(cx).unwrap();
        cx.subscribe(&ui, move |_, event, _| {
            if let crate::HostUiEvent::Toast { message, .. } = event {
                let _ = sender.try_send(message.clone());
            }
        })
        .detach();
    });
    call(cx, "peek", json!({})).unwrap();
    cx.run_until_parked();
    assert_eq!(toasts.try_recv().unwrap(), "Linear failed: [secret]");
}

/// A keychain kagan cannot reach in this run, as on Linux with no D-Bus answering.
struct Unreachable;

impl keyring_core::api::CredentialStoreApi for Unreachable {
    fn vendor(&self) -> String {
        "unreachable".into()
    }

    fn id(&self) -> String {
        "unreachable".into()
    }

    fn build(
        &self,
        _: &str,
        _: &str,
        _: Option<&std::collections::HashMap<&str, &str>>,
    ) -> keyring_core::Result<keyring_core::Entry> {
        Err(keyring_core::Error::NoDefaultStore)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[gpui_kit::test]
fn a_keychain_out_of_reach_keeps_the_names_of_the_secrets_it_holds(cx: &mut TestAppContext) {
    let plugin = secret_plugin();
    open(cx, &[installed(plugin.path())]);
    let mock = use_mock_keychain(cx);
    set(cx, json!(SECRET)).unwrap();
    let data = data_dir(cx);
    let item = keychain_item(&mock, &data, "token");
    cx.update(|cx| {
        crate::PluginHost::global(cx).update(cx, |host, _| {
            host.secrets = Store::new(Some(Arc::new(Unreachable)), host.data_dir.clone());
        });
    });

    let error = set(cx, json!("lin_api_newer")).unwrap_err().to_string();
    assert!(error.contains("cannot be reached"), "{error}");
    let OpResult::Warnings(warnings) = remove(cx) else {
        panic!("a warning names the secret the keychain still holds")
    };
    assert!(warnings[0].contains("token"), "{warnings:?}");
    assert_eq!(
        item.get_password().unwrap(),
        SECRET,
        "not moved to the file"
    );
    let kept = std::fs::read_to_string(data.join("secrets").join(format!("{ID}.json"))).unwrap();
    assert!(
        kept.contains("token") && !kept.contains("lin_api_newer"),
        "{kept}"
    );
}
