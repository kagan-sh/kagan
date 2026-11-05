use kg_domain::OpError;
use serde_json::{Value, json};

use crate::manifest::Grant;

use super::{
    ID, check_plugin, home, installed, plugin_dir, read_manifest, sample_manifest, unique,
    write_plugin, write_theme_files,
};

#[test]
fn manifests_reject_unknown_keys_bad_ids_and_newer_requirements() {
    let dir = plugin_dir();
    let rejected = |edit: &dyn Fn(&mut Value), expected: &str| {
        let mut value = sample_manifest();
        edit(&mut value);
        std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
        let error = read_manifest(dir.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    };
    rejected(&|value| value["extra"] = json!(1), "unknown field `extra`");
    rejected(
        &|value| value["downloads"] = json!({}),
        "unknown field `downloads`",
    );
    rejected(
        &|value| value["powers"]["spawn"] = json!([{ "program": "tool/bin/server" }]),
        "unknown field `spawn`",
    );
    rejected(
        &|value| value["powers"]["serve"] = json!(true),
        "unknown field `serve`",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["downloads"]),
        "unknown variant",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["agents"]),
        "unknown variant `agents`",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["notify"]),
        "unknown variant `notify`",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["hotkeys"]),
        "unknown variant `hotkeys`",
    );
    rejected(
        &|value| value["powers"]["exec"] = json!(true),
        "unknown field `exec`",
    );
    rejected(&|value| value["id"] = json!("echo"), "reverse-DNS");
    rejected(
        &|value| value["kagan"] = json!(">=9.0.0"),
        "needs kagan >=9.0.0",
    );
    rejected(&|value| value["entry"] = json!("../main.js"), "entry");
    rejected(
        &|value| value["tools"][0]["input"] = json!("/etc/passwd"),
        "inside the plugin",
    );
    rejected(
        &|value| value["tools"][1]["name"] = json!("echo"),
        "declared twice",
    );
    rejected(
        &|value| value["events"] = json!(["idle"]),
        "unknown event \"idle\"",
    );
    rejected(
        &|value| value["events"] = json!(["thread.nope"]),
        "unknown event \"thread.nope\"",
    );
    rejected(
        &|value| value["powers"]["areas"] = json!(["audio"]),
        "unknown variant",
    );
    let mut value = sample_manifest();
    value["powers"]["areas"] = json!(["threads"]);
    value["events"] = json!(["thread.idle", "thread.turn.ended"]);
    std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
    let read = read_manifest(dir.path()).unwrap();
    assert!(read.events.contains("thread.idle"));
    assert!(read.events.contains("thread.turn.ended"));
}

#[test]
fn settings_keep_the_order_written_in_the_file() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(
        dir.path().join("main.js"),
        "export default class Plugin {}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join(crate::manifest::FILE),
        r#"{
            "id": "dev.example.order",
            "name": "Order",
            "version": "1.0.0",
            "kagan": ">=0.1.0",
            "entry": "main.js",
            "settings": {
                "zeta": { "type": "boolean", "title": "Zeta" },
                "alpha": { "type": "string", "title": "Alpha" },
                "mid": { "type": "select", "title": "Mid", "options": ["b", "a"], "default": "b" }
            }
        }"#,
    )
    .unwrap();
    let read = read_manifest(dir.path()).unwrap();
    let keys: Vec<_> = read.settings.iter().map(|(key, _)| key.as_str()).collect();
    assert_eq!(keys, ["zeta", "alpha", "mid"]);
}

#[test]
fn a_read_entry_must_be_a_folder_under_home() {
    let dir = plugin_dir();
    let rejected = |entry: &str, expected: &str| {
        let mut value = sample_manifest();
        value["powers"]["read"] = json!([entry]);
        std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
        let error = read_manifest(dir.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "{entry}: {error}");
    };
    rejected("/tmp/notes", "written as ~/");
    rejected("~/../etc", "stay inside");
    rejected("~/.claude/../../etc", "stay inside");
    rejected("~/", "stay inside");
    rejected("~\\..", "written as ~/");
    rejected("C:\\x", "written as ~/");
    rejected("\\\\host\\share", "written as ~/");

    let home = home();
    let stamp = unique("file");
    let file = home.join(&stamp);
    std::fs::write(&file, "x").unwrap();
    rejected(&format!("~/{stamp}"), "must be a folder");
    std::fs::remove_file(&file).unwrap();

    let missing = format!("~/{}", unique("missing"));
    let mut value = sample_manifest();
    value["powers"]["read"] = json!([missing]);
    std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
    read_manifest(dir.path()).unwrap();

    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        let name = unique("link");
        let link = home.join(&name);
        std::os::unix::fs::symlink(outside.path(), &link).unwrap();
        rejected(&format!("~/{name}"), "outside");
        std::fs::remove_file(link).unwrap();
    }
}

#[test]
fn page_store_is_an_install_line_and_needs_approval_again() {
    let plugin = plugin_dir();
    let html = plugin.path().join("ui");
    std::fs::create_dir_all(&html).unwrap();
    std::fs::write(html.join("index.html"), "<p>hi</p>").unwrap();
    let base = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !base
            .lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later")
    );
    let mut value = sample_manifest();
    value["ui"]["panels"] = json!([{
        "id": "board",
        "title": "Board",
        "web": "ui/index.html",
        "placement": "right",
        "store": true
    }]);
    write_plugin(plugin.path(), &value);
    let with = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        with.lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later"),
        "{:?}",
        with.lines()
    );
    assert!(base.within(&with));
    assert!(!with.within(&base));
}

#[test]
fn open_url_is_an_install_line_and_needs_approval_again() {
    let plugin = plugin_dir();
    let base = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !base
            .lines()
            .iter()
            .any(|line| line == "opens links in your browser")
    );
    let mut value = sample_manifest();
    value["powers"]["openUrl"] = json!(true);
    write_plugin(plugin.path(), &value);
    let with = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        with.lines()
            .iter()
            .any(|line| line == "opens links in your browser"),
        "{:?}",
        with.lines()
    );
    assert!(base.within(&with));
    assert!(!with.within(&base));
}

#[test]
fn webview_store_true_in_the_script_asks_for_page_data() {
    let plugin = plugin_dir();
    std::fs::write(
        plugin.path().join("main.js"),
        r#"
import { View } from "gpui-kit";
import { WebView } from "gpui-component";
export default class Plugin extends View {
  render() {
    return new WebView("board").url("http://127.0.0.1/").store(true);
  }
}
"#,
    )
    .unwrap();
    let grant = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        grant
            .lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later"),
        "{:?}",
        grant.lines()
    );
}

#[test]
fn store_true_on_something_other_than_a_webview_asks_for_nothing() {
    let plugin = plugin_dir();
    std::fs::write(
        plugin.path().join("main.js"),
        r#"
import { View } from "gpui-kit";
export default class Plugin extends View {
  render() {
    const cache = new Map();
    cache.store(true);
    return null;
  }
}
"#,
    )
    .unwrap();
    let grant = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !grant
            .lines()
            .iter()
            .any(|line| line == "keeps page data on macOS 14 or later"),
        "{:?}",
        grant.lines()
    );
}

#[test]
fn a_grant_grows_only_when_it_asks_for_more() {
    let dir = plugin_dir();
    let grant = |edit: &dyn Fn(&mut Value)| {
        let mut value = sample_manifest();
        edit(&mut value);
        std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
        Grant::of(&read_manifest(dir.path()).unwrap(), dir.path())
    };
    let base = grant(&|value| {
        value["powers"]["fetch"] =
            json!([{ "host": "api.github.com", "methods": ["GET", "POST"] }]);
    });
    let fewer = grant(&|value| {
        value["powers"]["fetch"] = json!([{ "host": "api.github.com", "methods": ["GET"] }]);
        value["powers"]["clipboard"] = json!({});
    });
    let more = grant(&|value| {
        value["powers"]["fetch"] =
            json!([{ "host": "api.github.com", "methods": ["GET", "POST"] }]);
        value["powers"]["run"] = json!([{ "program": "gh" }]);
    });
    assert!(fewer.within(&base));
    assert!(!more.within(&base));
    assert_eq!(
        more.lines(),
        vec![
            "runs gh",
            "talks to api.github.com (GET, POST)",
            "writes to your clipboard",
            "reads a folder you choose (Notes)",
        ]
    );
}

#[test]
fn local_fetch_grants_http_on_that_port_and_install_line() {
    use gpui_shell::Capabilities;

    use crate::manifest::{Fetch, Method};

    let dir = plugin_dir();
    let mut value = sample_manifest();
    value["powers"]["fetch"] = json!([
        { "host": "http://127.0.0.1:3847", "methods": ["GET"] },
        { "host": "http://localhost:3847", "methods": ["POST"] },
        { "host": "api.github.com", "methods": ["GET"] }
    ]);
    std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
    let grant = Grant::of(&read_manifest(dir.path()).unwrap(), dir.path());
    assert_eq!(
        grant.lines(),
        vec![
            "talks to programs on this computer (port 3847)",
            "talks to api.github.com (GET)",
            "writes to your clipboard",
            "reads a folder you choose (Notes)",
        ]
    );

    let caps = Capabilities::new().http_requests(
        [
            Fetch {
                host: "http://127.0.0.1:3847".into(),
                methods: [Method::Get].into_iter().collect(),
            },
            Fetch {
                host: "api.github.com".into(),
                methods: [Method::Get].into_iter().collect(),
            },
        ]
        .iter()
        .map(|fetch| fetch.http_grant().unwrap()),
    );
    assert!(caps.may_request("http", "127.0.0.1", Some(3847), "GET", "/"));
    assert!(!caps.may_request("http", "127.0.0.1", Some(3848), "GET", "/"));
    assert!(!caps.may_request("http", "127.0.0.1", Some(3847), "POST", "/"));
    assert!(!caps.may_request("https", "127.0.0.1", Some(3847), "GET", "/"));
    assert!(caps.may_request("https", "api.github.com", None, "GET", "/"));
    assert!(!caps.may_request("http", "api.github.com", None, "GET", "/"));
}

#[test]
fn thread_events_need_the_threads_area() {
    let dir = plugin_dir();
    let mut value = sample_manifest();
    value["events"] = json!(["thread.message"]);
    std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
    let error = read_manifest(dir.path()).unwrap_err().to_string();
    assert!(error.contains("threads area"), "{error}");
}

#[test]
fn local_fetch_hosts_are_checked() {
    let dir = plugin_dir();
    let rejected = |host: &str, expected: &str| {
        let mut value = sample_manifest();
        value["powers"]["fetch"] = json!([{ "host": host, "methods": ["GET"] }]);
        std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
        let error = read_manifest(dir.path()).unwrap_err().to_string();
        assert!(error.contains(expected), "host {host:?}: {error}");
    };
    rejected("127.0.0.1", "http://127.0.0.1:<port>");
    rejected("localhost", "http://localhost:<port>");
    rejected("::1", "IPv6 is unsupported");
    rejected("[::1]", "IPv6 is unsupported");
    rejected("::ffff:127.0.0.1", "IPv6 is unsupported");
    rejected("[::ffff:127.0.0.1]", "IPv6 is unsupported");
    rejected("[::ffff:7f00:1]", "IPv6 is unsupported");
    rejected("0.0.0.0", "IPv6 is unsupported");
    rejected("[::]", "IPv6 is unsupported");
    rejected("[::ffff:0:0]", "IPv6 is unsupported");
    rejected("localhost.", "IPv6 is unsupported");
    rejected("foo.localhost", "IPv6 is unsupported");
    rejected("http://127.0.0.1", "explicit port");
    rejected("http://localhost", "explicit port");
    rejected("http://example.com:80", "only for 127.0.0.1 or localhost");
    rejected(
        "https://127.0.0.1:443",
        "hostname, or http://127.0.0.1:<port>",
    );
    rejected("http://[::1]:3847", "only for 127.0.0.1 or localhost");
    rejected("127.0.0.1:3847", "hostname, or http://127.0.0.1:<port>");
    rejected("example.com:443", "hostname, or http://127.0.0.1:<port>");

    for host in ["http://127.0.0.1:80", "http://localhost:80"] {
        let mut value = sample_manifest();
        value["powers"]["fetch"] = json!([{ "host": host, "methods": ["GET"] }]);
        std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
        let grant = Grant::of(&read_manifest(dir.path()).unwrap(), dir.path());
        assert!(
            grant.lines().iter().any(|line| line.contains("port 80")),
            "{host}: {:?}",
            grant.lines()
        );
    }

    let mut value = sample_manifest();
    value["powers"]["fetch"] = (1..=crate::manifest::LOCAL_FETCH_CAP + 1)
        .map(|port| json!({ "host": format!("http://127.0.0.1:{port}"), "methods": ["GET"] }))
        .collect();
    std::fs::write(dir.path().join(crate::manifest::FILE), value.to_string()).unwrap();
    let error = read_manifest(dir.path()).unwrap_err().to_string();
    assert!(
        error.contains(&format!("at most {}", crate::manifest::LOCAL_FETCH_CAP)),
        "{error}"
    );
}

#[test]
fn program_environment_grants_can_shrink_but_cannot_grow() {
    use crate::manifest::{Env, Program};

    let program = |env| Program {
        program: "git".into(),
        env,
    };
    let mut approved = Grant::default();
    approved.powers.run = vec![program(Some(Env::Inherit))];
    let mut asked = approved.clone();
    asked.powers.run = vec![program(None)];
    assert!(asked.within(&approved));
    assert!(!approved.within(&asked));
    assert_eq!(approved.intersect(&asked).powers.run, asked.powers.run);
    assert!(
        approved
            .lines()
            .iter()
            .any(|line| line == "runs git with your environment")
    );
}

#[test]
fn declared_skills_validate_paths_and_frontmatter() {
    let plugin = plugin_dir();
    let skill = plugin.path().join("skills/my-task");
    std::fs::create_dir_all(&skill).unwrap();
    let mut changed = sample_manifest();
    changed["skills"] = json!(["skills/my-task"]);
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        changed.to_string(),
    )
    .unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: my-task\ndescription: My task\n---\nInstructions",
    )
    .unwrap();
    check_plugin(plugin.path()).unwrap();
    std::fs::write(
        skill.join("SKILL.md"),
        "---\nname: wrong-name\ndescription: My task\n---\nInstructions",
    )
    .unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("must match")
    );
    changed["skills"] = json!(["../outside"]);
    std::fs::write(
        plugin.path().join(crate::manifest::FILE),
        changed.to_string(),
    )
    .unwrap();
    assert!(
        check_plugin(plugin.path())
            .unwrap_err()
            .to_string()
            .contains("must stay inside")
    );
}

#[test]
fn a_manifest_declares_theme_files_inside_the_plugin() {
    let plugin = plugin_dir();
    let _outside = write_theme_files(&plugin.path().join("themes"));
    let declare = |themes: Value| {
        let mut declared = sample_manifest();
        declared["themes"] = themes;
        write_plugin(plugin.path(), &declared);
        read_manifest(plugin.path()).and_then(|manifest| {
            manifest
                .themes(plugin.path())
                .map_err(|error| OpError::Invalid(error.to_string()))
        })
    };

    let read = declare(json!([{ "id": "midnight", "file": "themes/midnight.json" }])).unwrap();
    assert_eq!(read.len(), 1);
    assert_eq!(read[0].0, format!("{ID}/midnight"));
    assert_eq!(read[0].1.name, "Ocean");
    assert!(check_plugin(plugin.path()).is_ok());

    for (themes, message) in [
        (json!([{ "name": "midnight" }]), "unknown field `name`"),
        (
            json!([{ "id": "Mid Night", "file": "themes/midnight.json" }]),
            "theme id \"Mid Night\" must be unique",
        ),
        (
            json!([
                { "id": "midnight", "file": "themes/midnight.json" },
                { "id": "midnight", "file": "themes/midnight.json" }
            ]),
            "theme id \"midnight\" must be unique",
        ),
        (
            json!([{ "id": "away", "file": "../midnight.json" }]),
            "theme away file ../midnight.json: must be a path inside the plugin",
        ),
        (
            json!([{ "id": "gone", "file": "themes/gone.json" }]),
            "theme gone file themes/gone.json: ",
        ),
        (
            json!([{ "id": "blank", "file": "themes/blank.json" }]),
            "theme blank file themes/blank.json: theme name is empty",
        ),
        (
            json!([{ "id": "odd", "file": "themes/odd.json" }]),
            "theme odd file themes/odd.json: expected #RRGGBB",
        ),
        (
            json!([{ "id": "big", "file": "themes/big.json" }]),
            "theme big file themes/big.json: larger than 64 KiB",
        ),
        (
            json!([{ "id": "long", "file": "themes/long.json" }]),
            "theme long file themes/long.json: theme name must be one line of at most 64 characters",
        ),
        (
            json!([{ "id": "lines", "file": "themes/lines.json" }]),
            "theme lines file themes/lines.json: theme name must be one line",
        ),
        (
            json!([{ "id": "dir", "file": "themes/dir.json" }]),
            "theme dir file themes/dir.json: not a regular file",
        ),
    ]
    .into_iter()
    .chain(
        [
            (
                json!([{ "id": "link", "file": "themes/link.json" }]),
                "theme link file themes/link.json: must stay inside the plugin",
            ),
            (
                json!([{ "id": "pipe", "file": "themes/pipe.json" }]),
                "theme pipe file themes/pipe.json: not a regular file",
            ),
        ]
        .into_iter()
        .filter(|_| cfg!(unix)),
    ) {
        let error = declare(themes).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
        let checked = check_plugin(plugin.path()).unwrap_err().to_string();
        assert!(checked.contains(message), "{checked}");
    }
}

/// One good theme and one of each bad kind. The returned folder holds the target of a symlink
/// that leaves the plugin.

#[test]
fn an_approved_grant_reads_as_plain_lines() {
    let plugin = plugin_dir();
    let mut value = sample_manifest();
    value["powers"] = json!({ "areas": ["threads"] });
    write_plugin(plugin.path(), &value);
    let approved = installed(plugin.path()).approved;
    assert_eq!(
        crate::power_lines(&approved).unwrap(),
        [
            "reads, starts and changes your threads, can type into threads and press Enter, reads their screen text, and reads their changed files",
            "reads a folder you choose (Notes)",
        ]
    );
    assert!(crate::power_lines(&json!("x")).is_err());
}

#[test]
fn instruct_is_an_install_line_and_needs_approval_again() {
    let plugin = plugin_dir();
    let base = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        !base
            .lines()
            .iter()
            .any(|line| line.contains("instructions"))
    );
    let mut value = sample_manifest();
    value["instruct"] = json!(true);
    write_plugin(plugin.path(), &value);
    let with = Grant::of(&read_manifest(plugin.path()).unwrap(), plugin.path());
    assert!(
        with.lines()
            .iter()
            .any(|line| line == "adds instructions to a thread's program"),
        "{:?}",
        with.lines()
    );
    assert!(base.within(&with));
    assert!(!with.within(&base));
}
