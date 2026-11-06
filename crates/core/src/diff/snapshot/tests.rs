use std::fs;
use std::path::Path;
use std::process::Command;

use kg_domain::{DiffQuery, DiffScope, ThreadId};

use super::*;

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .expect("git");
    assert!(status.success(), "{args:?}");
}

fn repo(branch: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp");
    git(dir.path(), &["init", "-b", branch]);
    dir
}

fn commit_file(root: &Path, name: &str, body: &str) {
    fs::write(root.join(name), body).expect("write");
    git(root, &["add", name]);
    git(root, &["commit", "-m", name]);
}

fn query(scope: DiffScope) -> DiffQuery {
    DiffQuery {
        thread_id: ThreadId("thr_1".into()),
        scope,
        base: None,
        rev: None,
    }
}

#[test]
fn the_requested_base_wins_over_the_hint() {
    let dir = repo("main");
    assert_eq!(
        resolve_base(dir.path(), Some("dev"), Some("main")).unwrap(),
        "dev"
    );
    assert_eq!(
        resolve_base(dir.path(), None, Some("topic")).unwrap(),
        "topic"
    );
}

#[test]
fn an_empty_request_falls_back_to_the_hint() {
    let dir = repo("main");
    assert_eq!(
        resolve_base(dir.path(), Some(""), Some("topic")).unwrap(),
        "topic"
    );
}

#[test]
fn origin_head_names_the_base_when_nothing_is_asked() {
    let dir = repo("main");
    commit_file(dir.path(), "a.txt", "a\n");
    git(
        dir.path(),
        &["update-ref", "refs/remotes/origin/main", "HEAD"],
    );
    git(
        dir.path(),
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/main",
        ],
    );
    assert_eq!(resolve_base(dir.path(), None, None).unwrap(), "origin/main");
}

#[test]
fn main_is_the_base_when_origin_is_unset() {
    let dir = repo("main");
    commit_file(dir.path(), "a.txt", "a\n");
    assert_eq!(resolve_base(dir.path(), None, None).unwrap(), "main");
}

#[test]
fn a_repository_without_a_base_branch_is_refused() {
    let dir = repo("trunk");
    assert!(resolve_base(dir.path(), None, None).is_err());
}

#[test]
fn the_empty_tree_is_gits_known_empty_hash() {
    let dir = repo("main");
    assert_eq!(
        empty_tree(dir.path()).unwrap(),
        "4b825dc642cb6eb9a060e54bf8d69288fbee4904"
    );
}

#[test]
fn a_commit_review_compares_against_its_parent() {
    let dir = repo("main");
    commit_file(dir.path(), "a.txt", "a\n");
    let parent = git_text(dir.path(), &["rev-parse", "HEAD"], None).unwrap();
    commit_file(dir.path(), "b.txt", "b\n");
    let head = git_text(dir.path(), &["rev-parse", "HEAD"], None).unwrap();
    let mut query = query(DiffScope::Commit);
    query.rev = Some("HEAD".into());
    let (left, right, _) = ends(dir.path(), None, &query).unwrap();
    assert_eq!(left.tree, parent);
    assert_eq!(right.tree, head);
}

#[test]
fn a_root_commit_review_starts_from_the_empty_tree() {
    let dir = repo("main");
    commit_file(dir.path(), "a.txt", "a\n");
    let mut query = query(DiffScope::Commit);
    query.rev = Some("HEAD".into());
    let (left, _, _) = ends(dir.path(), None, &query).unwrap();
    assert_eq!(left.tree, empty_tree(dir.path()).unwrap());
}

#[test]
fn a_commit_review_needs_a_name() {
    let dir = repo("main");
    commit_file(dir.path(), "a.txt", "a\n");
    let mut query = query(DiffScope::Commit);
    query.rev = Some(String::new());
    let Err(error) = ends(dir.path(), None, &query) else {
        panic!("a nameless commit must be refused");
    };
    assert_eq!(error.to_string(), "Name the commit to review");
}

#[test]
fn indexed_paths_lists_what_the_index_holds() {
    let dir = repo("main");
    commit_file(dir.path(), "a.txt", "a\n");
    commit_file(dir.path(), "b.txt", "b\n");
    fs::remove_file(dir.path().join("a.txt")).expect("remove");
    let index = dir.path().join(".git/index");
    let paths = indexed_paths(dir.path(), &index).unwrap();
    assert_eq!(
        paths,
        BTreeSet::from([b"a.txt".to_vec(), b"b.txt".to_vec()])
    );
}

#[test]
fn a_missing_file_is_not_skipped() {
    let dir = tempfile::tempdir().expect("temp");
    assert!(skip(&dir.path().join("gone")).unwrap().is_none());
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_read_is_an_error() {
    let dir = tempfile::tempdir().expect("temp");
    fs::write(dir.path().join("file"), b"x").expect("write");
    assert!(skip(&dir.path().join("file/child")).is_err());
}

#[test]
fn a_folder_is_not_skipped() {
    let dir = tempfile::tempdir().expect("temp");
    assert!(skip(dir.path()).unwrap().is_none());
}

#[test]
fn a_file_at_the_limit_is_not_too_large() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("limit.txt");
    let limit = usize::try_from(FILE_LIMIT).expect("fits");
    fs::write(&path, vec![b'a'; limit]).expect("write");
    assert!(skip(&path).unwrap().is_none());
}

#[test]
fn a_file_over_the_limit_is_skipped_for_its_size() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("over.txt");
    let limit = usize::try_from(FILE_LIMIT).expect("fits");
    fs::write(&path, vec![b'a'; limit + 1]).expect("write");
    let skipped = skip(&path).unwrap().expect("skipped");
    assert!(skipped.too_large && !skipped.binary);
    assert_eq!(skipped.bytes, FILE_LIMIT + 1);
}

#[test]
fn a_nul_at_the_limit_makes_the_file_binary() {
    let dir = tempfile::tempdir().expect("temp");
    let path = dir.path().join("binary.bin");
    let limit = usize::try_from(FILE_LIMIT).expect("fits");
    let mut body = vec![b'a'; limit];
    *body.last_mut().expect("last") = 0;
    fs::write(&path, body).expect("write");
    let skipped = skip(&path).unwrap().expect("skipped");
    assert!(skipped.binary && !skipped.too_large);
}
