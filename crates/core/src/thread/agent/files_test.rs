use std::path::{Path, PathBuf};

use super::{exclude_cursor_rule, git_exclude, hide_from_git, link_root};

fn skill(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::create_dir_all(&path).unwrap();
    dunce::canonicalize(path).unwrap()
}

fn folder(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::create_dir_all(path.join(".git/info")).unwrap();
    path
}

fn names(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

#[cfg(unix)]
#[test]
fn link_root_links_the_skill_and_hides_only_that_link_from_git() {
    let dir = tempfile::tempdir().unwrap();
    let _source = skill(dir.path(), "source");
    let work = folder(dir.path(), "work");
    let root = ".agents/skills";

    let owned = link_root(&work, dir.path(), dir.path(), &names(&["kg-threads"]), root).unwrap();
    hide_from_git(&work, root, &owned).unwrap();
    assert_eq!(
        std::fs::read_link(work.join(".agents/skills/kg-threads")).unwrap(),
        dir.path().join("kg-threads")
    );
    let exclude = work.join(".git/info/exclude");
    assert_eq!(
        std::fs::read_to_string(&exclude).unwrap(),
        ".agents/skills/kg-threads\n"
    );

    let owned = link_root(&work, dir.path(), dir.path(), &names(&["kg-threads"]), root).unwrap();
    hide_from_git(&work, root, &owned).unwrap();
    assert_eq!(
        std::fs::read_to_string(&exclude)
            .unwrap()
            .matches(".agents/skills/kg-threads")
            .count(),
        1
    );
}

#[cfg(unix)]
#[test]
fn link_root_leaves_a_person_s_folder_or_unrelated_link_alone() {
    let dir = tempfile::tempdir().unwrap();
    let _source = skill(dir.path(), "source");
    let root = ".agents/skills";

    let folder_path = dir.path().join("folder");
    std::fs::create_dir_all(folder_path.join(".agents/skills/kg-threads")).unwrap();
    let owned = link_root(
        &folder_path,
        dir.path(),
        dir.path(),
        &names(&["kg-threads"]),
        root,
    )
    .unwrap();
    assert!(owned.is_empty());
    assert!(!folder_path.join(".agents/skills/kg-threads").is_symlink());

    let other_dir = tempfile::tempdir().unwrap();
    let other = skill(other_dir.path(), "other");
    let linked = dir.path().join("linked");
    std::fs::create_dir_all(linked.join(".agents/skills")).unwrap();
    std::os::unix::fs::symlink(&other, linked.join(".agents/skills/kg-threads")).unwrap();
    let owned = link_root(
        &linked,
        dir.path(),
        dir.path(),
        &names(&["kg-threads"]),
        root,
    )
    .unwrap();
    assert!(owned.is_empty());
    assert_eq!(
        std::fs::read_link(linked.join(".agents/skills/kg-threads")).unwrap(),
        other
    );
}

#[cfg(unix)]
#[test]
fn link_root_removes_a_kagan_link_the_catalog_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let _source = skill(dir.path(), "source");
    let work = folder(dir.path(), "work");
    std::fs::create_dir_all(work.join(".agents/skills")).unwrap();
    std::os::unix::fs::symlink(dir.path().join("gone"), work.join(".agents/skills/gone")).unwrap();

    let owned = link_root(
        &work,
        dir.path(),
        dir.path(),
        &names(&["kept"]),
        ".agents/skills",
    )
    .unwrap();
    hide_from_git(&work, ".agents/skills", &owned).unwrap();
    assert!(!work.join(".agents/skills/gone").exists());
    assert!(work.join(".agents/skills/kept").is_symlink());
}

#[cfg(unix)]
#[test]
fn hide_from_git_keeps_absent_lines_and_drops_a_real_folder_line() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(dir.path(), "work");
    let exclude = work.join(".git/info/exclude");
    std::fs::create_dir_all(work.join(".agents/skills/mine")).unwrap();
    std::fs::write(
        &exclude,
        "target\n.agents/skills/gone\n.agents/skills/mine\n.agents/skills/future\n",
    )
    .unwrap();

    hide_from_git(&work, ".agents/skills", &names(&["kept"])).unwrap();
    assert_eq!(
        std::fs::read_to_string(&exclude).unwrap(),
        "target\n.agents/skills/gone\n.agents/skills/future\n.agents/skills/kept\n"
    );
}

#[test]
fn hide_from_git_does_nothing_without_a_git_folder() {
    let dir = tempfile::tempdir().unwrap();
    hide_from_git(dir.path(), ".agents/skills", &names(&["one"])).unwrap();
    assert!(!dir.path().join(".git/info/exclude").exists());
}

#[test]
fn hide_from_git_refuses_an_exclude_it_cannot_read() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(dir.path(), "work");
    let exclude = work.join(".git/info/exclude");
    std::fs::remove_file(&exclude).ok();
    std::fs::create_dir(&exclude).unwrap();
    assert!(hide_from_git(&work, ".agents/skills", &names(&["one"])).is_err());
}

#[test]
fn kimi_re_adds_its_user_roots() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(dir.path().join(".agents/skills/probe")).unwrap();
    std::fs::create_dir_all(dir.path().join(".kimi-code/skills/probe")).unwrap();
    let home_var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    let previous_home = std::env::var_os(home_var);
    let previous_kimi = std::env::var_os("KIMI_CODE_HOME");
    // SAFETY: nextest runs each test in its own process, so this test owns these
    // process-global variables; the unsafe block below puts them back.
    unsafe {
        std::env::set_var(home_var, dir.path());
        std::env::remove_var("KIMI_CODE_HOME");
    }
    let args = super::kimi_skills(&dir.path().join("catalog"), dir.path(), None);
    // SAFETY: see above.
    unsafe {
        match previous_home {
            Some(value) => std::env::set_var(home_var, value),
            None => std::env::remove_var(home_var),
        }
        match previous_kimi {
            Some(value) => std::env::set_var("KIMI_CODE_HOME", value),
            None => std::env::remove_var("KIMI_CODE_HOME"),
        }
    }
    let dirs = args
        .chunks(2)
        .map(|pair| pair[1].clone())
        .collect::<Vec<_>>();
    assert!(
        dirs.iter()
            .any(|arg| arg == &dir.path().join(".agents/skills").into_os_string())
    );
    assert!(
        dirs.iter()
            .any(|arg| arg == &dir.path().join(".kimi-code/skills").into_os_string())
    );
}

#[test]
fn a_linked_worktree_uses_the_common_exclude() {
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path().join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    let git = |cwd: &Path, args: &[&str]| {
        let status = std::process::Command::new("git")
            .args(args)
            .current_dir(cwd)
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    };
    git(&repo, &["init"]);
    std::fs::write(repo.join("README.md"), "test\n").unwrap();
    git(&repo, &["add", "README.md"]);
    git(
        &repo,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.test",
            "commit",
            "-m",
            "initial",
        ],
    );
    let worktree = dir.path().join("worktree");
    git(
        &repo,
        &["worktree", "add", worktree.to_str().unwrap(), "-b", "test"],
    );

    let exclude = dunce::canonicalize(git_exclude(&worktree).unwrap()).unwrap();
    assert_eq!(
        exclude,
        dunce::canonicalize(repo.join(".git/info/exclude")).unwrap()
    );
    std::fs::create_dir_all(worktree.join(".agents/skills")).unwrap();
    let second = dir.path().join("worktree2");
    git(
        &repo,
        &["worktree", "add", second.to_str().unwrap(), "-b", "test2"],
    );
    std::fs::create_dir_all(second.join(".agents/skills")).unwrap();
    std::fs::write(second.join(".agents/skills/foo"), "foo\n").unwrap();
    std::fs::write(&exclude, ".agents/skills/one\n.agents/skills/foo\n").unwrap();

    // A start in the first worktree adds its own line and leaves the second
    // worktree's line alone, so its link stays hidden.
    hide_from_git(&worktree, ".agents/skills", &names(&["bar"])).unwrap();
    assert!(
        std::fs::read_to_string(&exclude)
            .unwrap()
            .contains(".agents/skills/foo")
    );
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&worktree)
        .output()
        .unwrap();
    assert!(
        status.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
    let status = std::process::Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(&second)
        .output()
        .unwrap();
    assert!(
        status.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&status.stdout)
    );
}

#[test]
fn git_exclude_follows_a_gitdir_file_and_skips_a_folder_without_git() {
    let dir = tempfile::tempdir().unwrap();
    let real = dir.path().join("real");
    std::fs::create_dir_all(real.join("info")).unwrap();
    let tree = dir.path().join("tree");
    std::fs::create_dir_all(&tree).unwrap();
    std::fs::write(tree.join(".git"), format!("gitdir: {}\n", real.display())).unwrap();
    assert_eq!(git_exclude(&tree), Some(real.join("info/exclude")));

    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    assert_eq!(git_exclude(&plain), None);
}

#[test]
fn a_cursor_rule_exclude_survives_a_real_file() {
    let dir = tempfile::tempdir().unwrap();
    let work = folder(dir.path(), "work");
    let rule = work.join(".cursor/rules/kagan.mdc");
    std::fs::create_dir_all(rule.parent().unwrap()).unwrap();
    std::fs::write(&rule, "rule\n").unwrap();
    let stale = work.join(".agents/skills/stale");
    std::fs::create_dir_all(&stale).unwrap();
    std::fs::write(work.join(".git/info/exclude"), ".agents/skills/stale\n").unwrap();
    exclude_cursor_rule(&work).unwrap();
    exclude_cursor_rule(&work).unwrap();
    hide_from_git(&work, ".agents/skills", &names(&["kg-threads"])).unwrap();
    let exclude = std::fs::read_to_string(work.join(".git/info/exclude")).unwrap();
    assert_eq!(
        exclude
            .lines()
            .filter(|line| line.trim() == ".cursor/rules/kagan.mdc")
            .count(),
        1,
        "{exclude}"
    );
    assert!(exclude.contains(".agents/skills/kg-threads"), "{exclude}");
    assert!(
        !exclude
            .lines()
            .any(|line| line.trim() == ".agents/skills/stale"),
        "{exclude}"
    );
}

#[test]
fn a_folder_without_git_skips_the_cursor_exclude() {
    let dir = tempfile::tempdir().unwrap();
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&plain).unwrap();
    exclude_cursor_rule(&plain).unwrap();
    assert!(!plain.join(".git").exists());
}
