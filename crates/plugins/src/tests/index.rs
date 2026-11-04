use std::path::Path;

use kg_domain::IndexEntry;
use serde_json::{Value, json};

use crate::index::{names_an_index, origin, parse, public, read, shown, source_place};

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
