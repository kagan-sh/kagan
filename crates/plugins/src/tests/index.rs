use std::path::Path;

use gpui_kit::TestAppContext;
use kg_domain::{IndexEntry, IndexOp, Op, OpError, OpResult, PluginId, PluginOp};
use serde_json::{Value, json};
use tempfile::TempDir;

use crate::index::{names_an_index, origin, parse, public, read, shown, source_place};
use crate::source;

use super::{
    ID, Indexed, answer, git, index_op, indexed, install, local_index, open, open_kept, plugin_op,
    records, run, sample_manifest, search, spare_data, thread, write_plugin,
};

fn index(plugins: &Value) -> String {
    json!({ "schemaVersion": 1, "name": "acme", "plugins": plugins }).to_string()
}

fn refused(text: &str, folder: Option<&Path>) -> String {
    match parse(text, folder) {
        Ok(listings) => panic!("parsed {listings:?}"),
        Err(error) => error.to_string(),
    }
}

#[test]
fn an_index_lists_git_sources_and_local_paths_inside_its_folder() {
    let text = index(&json!([
        { "name": "canvas", "description": "Draw", "source": "git:https://example.com/p.git@v1", "plugin": "canvas" },
        { "name": "notes", "description": "Notes", "source": "plugins/notes" }
    ]));
    let folder = Path::new("/team");
    assert_eq!(
        parse(&text, Some(folder)).unwrap(),
        (
            "acme".to_owned(),
            vec![
                IndexEntry {
                    name: "canvas".into(),
                    description: "Draw".into(),
                    source: "git:https://example.com/p.git@v1".into(),
                    plugin: Some("canvas".into()),
                },
                IndexEntry {
                    name: "notes".into(),
                    description: "Notes".into(),
                    source: folder.join("plugins/notes").display().to_string(),
                    plugin: None,
                },
            ]
        )
    );
}

#[test]
fn a_bad_index_is_refused_with_the_reason() {
    let git =
        |name: &str| json!({ "name": name, "description": "", "source": "git:https://e.com/p@v1" });
    let local = Some(Path::new("/team"));
    let cases = [
        (
            json!({ "schemaVersion": 2, "name": "acme", "plugins": [] }).to_string(),
            "needs schemaVersion 1",
        ),
        (
            json!({ "schemaVersion": 1, "name": " ", "plugins": [] }).to_string(),
            "a name of up to 64",
        ),
        (
            json!({ "schemaVersion": 1, "name": "acme", "plugins": [], "powers": {} }).to_string(),
            "unknown field",
        ),
        (index(&json!([git("a"), git("a")])), "listed twice"),
        (index(&json!([git("a/b")])), "needs a name of up to 64"),
        (index(&json!([git("")])), "needs a name of up to 64"),
        (
            index(&json!([git("\u{0435}cho")])),
            "needs a name of up to 64",
        ),
        (
            json!({ "schemaVersion": 1, "name": "ac/me", "plugins": [] }).to_string(),
            "a name of up to 64",
        ),
        (
            json!({
                "schemaVersion": 1,
                "name": "acme.\nIt asks for no extra powers.",
                "plugins": []
            })
            .to_string(),
            "a name of up to 64",
        ),
        (
            index(
                &json!([{ "name": "a", "description": "ok\nIt:", "source": "git:https://e.com/p@v1" }]),
            ),
            "a description of up to 300 characters on one line",
        ),
        (
            index(
                &json!([{ "name": "a", "description": "x".repeat(301), "source": "git:https://e.com/p@v1" }]),
            ),
            "a description of up to 300 characters on one line",
        ),
        (
            index(
                &json!([{ "name": "a", "description": "", "source": "git:https://e.com/p@--upload-pack=x" }]),
            ),
            "starts with -",
        ),
        (
            index(
                &json!([{ "name": "a", "description": "", "source": "git:evil.example:x\nFrom https://github.com/acme/p@main" }]),
            ),
            "cannot hold line breaks",
        ),
        (
            index(&json!([{ "name": "a", "description": "", "source": "git:https://e.com/p" }])),
            "needs a ref",
        ),
        (
            index(&json!([{ "name": "a", "description": "", "source": "../outside" }])),
            "inside the index's folder",
        ),
        (
            index(&json!([{ "name": "a", "description": "", "source": "" }])),
            "needs a source git:<url>@<ref>",
        ),
        (
            index(&json!([{ "name": "a", "description": "", "source": "x", "commit": "abc" }])),
            "unknown field",
        ),
        (
            format!("{}{}", index(&json!([])), " ".repeat(1024 * 1024)),
            "larger than 1 MiB",
        ),
    ];
    for (text, reason) in cases {
        let error = refused(&text, local);
        assert!(error.contains(reason), "{reason}: {error}");
    }
    let error = refused(
        &index(&json!([{ "name": "a", "description": "", "source": "plugins/a" }])),
        None,
    );
    assert!(error.contains("needs a source git:<url>@<ref>"), "{error}");
}

#[test]
fn a_bare_name_is_looked_up_and_a_path_or_git_source_is_not() {
    assert!(names_an_index("canvas"));
    assert!(!names_an_index("git:https://e.com/p@v1"));
    let absolute = std::env::temp_dir().join("canvas");
    assert!(!names_an_index(&absolute.display().to_string()));
}

#[test]
fn the_card_names_the_address_ref_and_commit_and_cautions_off_an_index_host_and_owner() {
    let commit = "0123456789abcdef0123456789abcdef01234567";
    let from = format!("From https://github.com/acme/p at v1, commit {commit}.");
    let source = "git:https://someone:token@github.com/acme/p@v1";
    let acme = ["git:git@github.com:Acme/index@main".to_owned()];
    assert_eq!(
        origin(source, Path::new("/x"), Some(commit), None, &acme),
        std::slice::from_ref(&from)
    );
    assert_eq!(
        origin(
            "git:https://github.com/evil/p@v1",
            Path::new("/x"),
            Some(commit),
            None,
            &acme
        )[1],
        "Caution: no index you added lives at github.com/evil."
    );
    assert_eq!(
        origin(
            source,
            Path::new("/x"),
            Some(commit),
            None,
            &["git:https://gitlab.com/acme/index@main".into()]
        ),
        [
            from,
            "Caution: no index you added lives at github.com/acme.".into()
        ]
    );
    assert_eq!(
        origin("/team/echo", Path::new("/team/echo"), None, None, &[]),
        [
            "From the folder /team/echo.".to_owned(),
            "Caution: a folder on this computer is not pinned to a commit.".into()
        ]
    );
    let place = |source: &str| source_place(source).map(|(host, owner)| format!("{host}/{owner}"));
    assert_eq!(
        place("git:git@GitHub.com:O/r@v1").as_deref(),
        Some("github.com/o")
    );
    assert_eq!(
        place("git:ssh://git@example.com/o/r@v1").as_deref(),
        Some("example.com/o")
    );
    assert_eq!(place("git:/home/me/repo@main"), None);
    assert_eq!(place("git:file:///home/me/repo@main"), None);
}

#[test]
fn an_ssh_user_stays_and_an_https_user_and_any_password_go() {
    assert_eq!(
        public("ssh://git@github.com/o/r"),
        "ssh://git@github.com/o/r"
    );
    assert_eq!(
        public("ssh://git:secret@github.com/o/r"),
        "ssh://git@github.com/o/r"
    );
    assert_eq!(
        public("https://me:ghp_x@github.com/o/r"),
        "https://github.com/o/r"
    );
    assert_eq!(
        shown("git:https://me:ghp_x@github.com/o/index@main"),
        "git:https://github.com/o/index@main"
    );
}

#[test]
fn a_plain_web_address_is_not_an_index() {
    let (_keep, never) = async_channel::bounded::<()>(1);
    let Err(error) = read("https://example.com/index.json", &never) else {
        panic!("read a web file")
    };
    assert!(
        error.to_string().contains("does not read a plain web file"),
        "{error}"
    );
}

#[test]
fn a_shown_source_has_no_credentials() {
    assert_eq!(
        crate::index::shown("git:https://user:token@example.com/acme/plugin@main"),
        "git:https://example.com/acme/plugin@main"
    );
    assert_eq!(
        crate::index::shown("git:ssh://git@example.com/acme/plugin@main"),
        "git:ssh://git@example.com/acme/plugin@main"
    );
    assert_eq!(crate::index::shown("/local/plugin"), "/local/plugin");
}

#[test]
fn an_index_of_exactly_the_cap_is_read() {
    let base = index(&json!([]));
    let text = format!("{base}{}", " ".repeat(1024 * 1024 - base.len()));
    assert_eq!(text.len(), 1024 * 1024);
    assert!(parse(&text, None).is_ok());
}

#[cfg(unix)]
#[test]
fn a_git_index_of_exactly_the_cap_is_read() {
    let repo = tempfile::tempdir().unwrap();
    git(
        repo.path(),
        &["-c", "init.defaultBranch=main", "init", "-q"],
    );
    let base = index(&json!([]));
    let text = format!("{base}{}", " ".repeat(1024 * 1024 - base.len()));
    std::fs::create_dir(repo.path().join(".kagan")).unwrap();
    std::fs::write(repo.path().join(".kagan/index.json"), text).unwrap();
    git(repo.path(), &["add", "."]);
    git(repo.path(), &["commit", "-q", "-m", "index"]);
    let (_keep, stop) = async_channel::bounded::<()>(1);
    let location = format!("git:{}@main", repo.path().display());
    assert!(crate::install::index::read(&location, &stop).is_ok());
}

#[test]
fn repo_index_selects_named_plugin_and_refuses_ambiguous_or_escaping_sources() {
    let (_keep, stop) = async_channel::bounded::<()>(1);
    let repo = tempfile::tempdir().unwrap();
    write_plugin(&repo.path().join("plugins/echo"), &sample_manifest());
    std::fs::create_dir(repo.path().join(".kagan")).unwrap();
    let path = repo.path().join(".kagan/plugins.json");
    let index = |source: &str| json!({ "schemaVersion": 1, "name": "Test", "plugins": [{ "name": "named", "source": source }] });
    std::fs::write(&path, index("./plugins/echo").to_string()).unwrap();
    let source = source::Source::Local(repo.path().into());
    let plugins = tempfile::tempdir().unwrap();
    let selected = source::fetch(
        &source,
        Some(Path::new("named")),
        plugins.path(),
        spare_data(),
        &stop,
    )
    .unwrap();
    assert!(selected.root.ends_with("plugins/echo"));
    assert!(
        source::fetch(
            &source,
            Some(Path::new("plugins/echo")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .unwrap()
        .root
        .ends_with("plugins/echo")
    );
    assert!(
        source::fetch(
            &source,
            Some(Path::new("../plugins/echo")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .is_err()
    );
    std::fs::write(&path, index("plugins/echo").to_string()).unwrap();
    assert!(
        source::fetch(
            &source,
            Some(Path::new("./plugins/echo")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .unwrap()
        .root
        .ends_with("plugins/echo")
    );
    assert!(source::fetch(&source, None, plugins.path(), spare_data(), &stop).is_ok());
    assert!(
        source::fetch(
            &source,
            Some(Path::new("unknown")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .is_err()
    );
    let mut ambiguous = index("./plugins/echo");
    ambiguous["plugins"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "name": "second", "source": "./plugins/echo" }));
    std::fs::write(&path, ambiguous.to_string()).unwrap();
    assert!(source::fetch(&source, None, plugins.path(), spare_data(), &stop).is_err());
    std::fs::write(&path, index("../outside").to_string()).unwrap();
    assert!(
        source::fetch(
            &source,
            Some(Path::new("named")),
            plugins.path(),
            spare_data(),
            &stop
        )
        .is_err()
    );
    #[cfg(unix)]
    {
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), repo.path().join("plugins/out")).unwrap();
        std::fs::write(&path, index("./plugins/out").to_string()).unwrap();
        match source::fetch(
            &source,
            Some(Path::new("plugins/out")),
            plugins.path(),
            spare_data(),
            &stop,
        ) {
            Err(error) => assert!(error.to_string().contains("escapes"), "{error}"),
            Ok(_) => panic!("outward symlink was accepted"),
        }
    }
}

#[gpui_kit::test]
fn indexes_are_added_in_order_searched_and_an_unreadable_one_is_a_warning(cx: &mut TestAppContext) {
    let Indexed {
        source,
        listed,
        missing,
        data,
        _repos,
        ..
    } = indexed(cx);
    let add = |location: &str| {
        index_op(IndexOp::Add {
            location: location.into(),
        })
    };

    assert_eq!(
        run(cx, add(&listed)).unwrap(),
        OpResult::Indexes(vec![missing.clone(), listed.clone()])
    );
    let again = run(cx, add(&listed)).unwrap_err();
    assert!(again.to_string().contains("already added"), "{again}");
    assert!(
        run(cx, add(&missing.replace("missing", "gone"))).is_err(),
        "an index that cannot be read is not added"
    );

    let (listings, warnings) = search(cx, "ECHOES");
    assert_eq!(
        listings
            .iter()
            .map(|listing| (listing.name.as_str(), listing.source.as_str()))
            .collect::<Vec<_>>(),
        [("echo", source.as_str())]
    );
    let [warning] = warnings.as_slice() else {
        panic!("{warnings:?}")
    };
    assert!(
        warning.starts_with(&format!("could not read index {missing}")),
        "{warning}"
    );
    assert!(search(cx, "canvas").0.is_empty());

    let error = run(cx, Op::Plugin(install("echo".into(), Some(thread()), None))).unwrap_err();
    assert!(
        error
            .to_string()
            .contains(&format!("could not read index {missing}")),
        "an earlier index that cannot be read stops the lookup: {error}"
    );
    run(cx, index_op(IndexOp::Remove { location: missing })).unwrap();
    assert_eq!(
        std::fs::read_to_string(data.join("indexes.json"))
            .unwrap()
            .replace([' ', '\n'], ""),
        json!([listed]).to_string()
    );
}

#[gpui_kit::test]
fn install_by_name_reads_the_index_and_still_asks_a_person(cx: &mut TestAppContext) {
    let Indexed {
        source,
        commit,
        listed,
        missing,
        _repos,
        ..
    } = indexed(cx);
    run(cx, index_op(IndexOp::Remove { location: missing })).unwrap();
    run(
        cx,
        index_op(IndexOp::Add {
            location: listed.clone(),
        }),
    )
    .unwrap();
    let nothing = run(
        cx,
        Op::Plugin(install("nothing".into(), Some(thread()), None)),
    )
    .unwrap_err();
    assert!(
        nothing
            .to_string()
            .contains("no index lists a plugin named nothing"),
        "{nothing}"
    );

    let other = run(
        cx,
        Op::Plugin(install("other/echo".into(), Some(thread()), None)),
    );
    assert!(other.is_err_and(|error| {
        error
            .to_string()
            .contains("no index lists a plugin named other/echo")
    }));

    let declined = plugin_op(cx, install("acme/echo".into(), Some(thread()), None));
    assert_eq!(
        answer(cx, "decline"),
        format!(
            "Install Echo 1.0.0 ({ID})?\nFrom {} at main, commit {commit}, listed by index acme at {listed}.\nCaution: this repository is on this computer, not where an index you added lives.\nIt:\n- writes to your clipboard\n- reads a folder you choose (Notes)",
            source.trim_start_matches("git:").trim_end_matches("@main")
        )
    );
    assert!(cx.foreground_executor().block_test(declined).is_err());
    assert!(records(cx).is_empty());

    let approved = plugin_op(cx, install("echo".into(), Some(thread()), None));
    answer(cx, "approve");
    cx.foreground_executor().block_test(approved).unwrap();
    let [record] = records(cx).try_into().unwrap();
    assert_eq!(
        record.source, source,
        "the record keeps the git source, not the name"
    );
    assert_eq!(record.commit.as_deref(), Some(commit.as_str()));
    let again = run(cx, Op::Plugin(install("echo".into(), Some(thread()), None))).unwrap_err();
    assert!(
        again.to_string().contains("already installed"),
        "an index cannot point an installed plugin at another repository: {again}"
    );
    let index = Path::new(listed.trim_start_matches("git:").trim_end_matches("@main"));
    std::fs::write(
        index.join(".kagan/index.json"),
        json!({
            "schemaVersion": 1,
            "name": "acme",
            "plugins": [{ "name": "echo", "description": "Moved", "source": "git:https://example.com/evil/echo@main" }]
        })
        .to_string(),
    )
    .unwrap();
    git(index, &["commit", "--quiet", "-am", "moved"]);
    let update = PluginOp::Update {
        target: kg_domain::UpdateTarget::One(PluginId(ID.into())),
        thread_id: None,
    };
    assert_eq!(
        run(cx, Op::Plugin(update)).unwrap(),
        OpResult::Done,
        "update reads the recorded source, not the index"
    );
    assert_eq!(records(cx)[0].source, source);
}

#[gpui_kit::test]
fn share_refuses_a_plugin_that_is_not_in_a_git_repository(cx: &mut TestAppContext) {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("echo");
    write_plugin(&folder, &sample_manifest());
    open(cx, &[]);
    let error = run(
        cx,
        Op::Plugin(PluginOp::Share {
            path: folder.as_path().into(),
        }),
    )
    .unwrap_err();
    assert_eq!(
        error,
        OpError::Invalid(format!("{} is not in a git repository", folder.display()))
    );
    let repo = dir.path().join("repo");
    std::fs::create_dir(&repo).unwrap();
    git(&repo, &["init", "-q"]);
    let inside_git = repo.join(".git").join("echo");
    write_plugin(&inside_git, &sample_manifest());
    let error = run(
        cx,
        Op::Plugin(PluginOp::Share {
            path: inside_git.as_path().into(),
        }),
    )
    .unwrap_err();
    assert_eq!(
        error,
        OpError::Invalid(format!(
            "{} is not in a git repository",
            inside_git.display()
        ))
    );
}

#[gpui_kit::test]
fn share_prints_an_entry_that_pins_the_pushed_commit_and_installs_through_approval(
    cx: &mut TestAppContext,
) {
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "--quiet", "--bare"]);
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "--quiet", "--initial-branch=main"]);
    let folder = work.path().join("echo");
    write_plugin(&folder, &sample_manifest());
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "--quiet", "-m", "one"]);
    let commit = git(work.path(), &["rev-parse", "HEAD"]);
    let url = remote.path().display().to_string();
    git(work.path(), &["remote", "add", "origin", &url]);
    open(cx, &[]);
    let share = |path: &Path| Op::Plugin(PluginOp::Share { path: path.into() });

    let unpushed = run(cx, share(&folder)).unwrap_err();
    assert!(unpushed.to_string().contains("push commit"), "{unpushed}");
    git(work.path(), &["push", "--quiet", "origin", "main"]);
    std::fs::write(folder.join("notes.txt"), "draft").unwrap();
    let changed = run(cx, share(&folder)).unwrap_err();
    assert!(
        changed.to_string().contains("commit the changes"),
        "{changed}"
    );
    std::fs::remove_file(folder.join("notes.txt")).unwrap();

    let Ok(OpResult::Shared { entry, install }) = run(cx, share(&folder)) else {
        panic!("share")
    };
    let source = format!("git:{url}@{commit}");
    assert_eq!(
        serde_json::to_value(&entry).unwrap(),
        json!({ "name": "echo", "description": "Echo things back.", "source": source, "plugin": "echo" })
    );
    assert_eq!(install, format!("kg plugin install {source} --plugin echo"));
    let index = json!({ "schemaVersion": 1, "name": "acme", "plugins": [entry] }).to_string();
    assert_eq!(
        crate::index::parse(&index, None).unwrap().1[0].source,
        source
    );

    let installing = plugin_op(cx, self::install(source, Some(thread()), Some("echo")));
    assert_eq!(
        answer(cx, "approve"),
        format!(
            "Install Echo 1.0.0 ({ID})?\nFrom {url} at {commit}, commit {commit}.\nCaution: this repository is on this computer, not where an index you added lives.\nIt:\n- writes to your clipboard\n- reads a folder you choose (Notes)"
        )
    );
    cx.foreground_executor().block_test(installing).unwrap();
    assert_eq!(records(cx)[0].commit.as_deref(), Some(commit.as_str()));
}

#[gpui_kit::test]
fn an_index_cannot_fake_card_lines_or_take_another_index_name(cx: &mut TestAppContext) {
    let (first, second, faker) = (
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let echo = json!([{ "name": "echo", "description": "Echo", "source": "git:https://example.com/evil/echo@main" }]);
    local_index(first.path(), "acme", &json!([]));
    local_index(second.path(), "other", &echo);
    local_index(
        faker.path(),
        "acme.\nIt asks for no extra powers.",
        &json!([]),
    );
    open(cx, &[]);
    let add = |dir: &TempDir| {
        index_op(IndexOp::Add {
            location: dir.path().display().to_string(),
        })
    };

    let lines = run(cx, add(&faker)).unwrap_err();
    assert!(lines.to_string().contains("a name of up to 64"), "{lines}");
    run(cx, add(&first)).unwrap();
    local_index(faker.path(), "acme", &echo);
    let taken = run(cx, add(&faker)).unwrap_err();
    assert!(
        taken
            .to_string()
            .contains("two indexes cannot share a name"),
        "{taken}"
    );

    run(cx, add(&second)).unwrap();
    local_index(second.path(), "acme", &echo);
    let hijack = run(
        cx,
        Op::Plugin(install("acme/echo".into(), Some(thread()), None)),
    )
    .unwrap_err();
    assert!(
        hijack
            .to_string()
            .contains("no index lists a plugin named acme/echo"),
        "a later index that renames itself acme is not acme: {hijack}"
    );
    let (listings, warnings) = search(cx, "");
    assert!(listings.is_empty(), "{listings:?}");
    assert!(warnings[0].contains("is also named acme"), "{warnings:?}");

    let pinned = run(
        cx,
        Op::Plugin(install("echo".into(), Some(thread()), Some("other"))),
    )
    .unwrap_err();
    assert!(pinned.to_string().contains("--plugin is for"), "{pinned}");
}

#[gpui_kit::test]
fn a_saved_list_that_cannot_be_read_is_not_overwritten(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().unwrap();
    std::fs::write(data.path().join("indexes.json"), "{ not json").unwrap();
    let team = tempfile::tempdir().unwrap();
    local_index(team.path(), "team", &json!([]));
    let path = data.path().to_path_buf();
    open_kept(cx, &path, data, &[]);

    let error = run(
        cx,
        index_op(IndexOp::Add {
            location: team.path().display().to_string(),
        }),
    )
    .unwrap_err();
    assert!(error.to_string().contains("cannot be read"), "{error}");
    assert_eq!(
        std::fs::read_to_string(path.join("indexes.json")).unwrap(),
        "{ not json"
    );
}

#[gpui_kit::test]
fn share_keeps_the_ssh_user_and_drops_a_token(cx: &mut TestAppContext) {
    let work = tempfile::tempdir().unwrap();
    git(work.path(), &["init", "--quiet", "--initial-branch=main"]);
    write_plugin(work.path(), &sample_manifest());
    git(work.path(), &["add", "."]);
    git(work.path(), &["commit", "--quiet", "-m", "one"]);
    let commit = git(work.path(), &["rev-parse", "HEAD"]);
    git(
        work.path(),
        &[
            "remote",
            "add",
            "origin",
            "ssh://git:hunter2@example.com/acme/echo",
        ],
    );
    git(
        work.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    open(cx, &[]);

    let Ok(OpResult::Shared { entry, install }) = run(
        cx,
        Op::Plugin(PluginOp::Share {
            path: work.path().into(),
        }),
    ) else {
        panic!("share")
    };
    let source = format!("git:ssh://git@example.com/acme/echo@{commit}");
    assert_eq!(entry.source, source);
    assert_eq!(entry.plugin, None);
    assert_eq!(install, format!("kg plugin install {source}"));
}
