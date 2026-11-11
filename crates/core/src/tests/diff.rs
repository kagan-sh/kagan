use super::{Fixture, commit, diff_paths, git_in, op, program};
use gpui_kit::TestAppContext;
use kg_domain::*;

#[gpui_kit::test]
async fn a_diff_lists_the_uncommitted_file_and_the_branch_commit(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    commit(root, &["commit", "--allow-empty", "-m", "base"]);
    std::fs::write(root.join("a.txt"), "one\n").unwrap();
    git_in(root, &["add", "a.txt"]);
    commit(root, &["commit", "-m", "one"]);
    git_in(root, &["checkout", "-b", "feature"]);
    std::fs::write(root.join("a.txt"), "two\n").unwrap();
    commit(root, &["commit", "-am", "two"]);
    std::fs::write(root.join("b.txt"), "new\n").unwrap();
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!()
    };
    let uncommitted = diff_paths(&started.id, DiffScope::Uncommitted, cx).await;
    assert!(
        uncommitted.iter().any(|path| path == "b.txt"),
        "{uncommitted:?}"
    );
    let branch = diff_paths(&started.id, DiffScope::Branch, cx).await;
    assert!(branch.iter().any(|path| path == "a.txt"), "{branch:?}");
}

#[gpui_kit::test]
async fn a_staged_deletion_stays_in_the_changes(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    commit(root, &["commit", "--allow-empty", "-m", "base"]);
    std::fs::write(root.join("old.txt"), "gone\n").unwrap();
    std::fs::write(root.join("keep.txt"), "stay\n").unwrap();
    git_in(root, &["add", "old.txt", "keep.txt"]);
    commit(root, &["commit", "-m", "add"]);
    std::fs::write(root.join("keep.txt"), "changed\n").unwrap();
    git_in(root, &["rm", "old.txt"]);
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    let OpResult::Diff(list) = op(
        Op::Diff(DiffOp::List {
            query: DiffQuery {
                thread_id: started.id,
                scope: DiffScope::Uncommitted,
                base: None,
                rev: None,
            },
        }),
        cx,
    )
    .await
    .unwrap() else {
        panic!("diff")
    };
    let deleted = list
        .files
        .iter()
        .find(|file| file.path == "old.txt")
        .unwrap_or_else(|| panic!("{list:?}"));
    assert_eq!(deleted.status, DiffStatus::Deleted);
    let kept = list
        .files
        .iter()
        .find(|file| file.path == "keep.txt")
        .unwrap_or_else(|| panic!("{list:?}"));
    assert_eq!(kept.status, DiffStatus::Modified);
}

#[gpui_kit::test]
async fn diff_revision_changes_for_binary_large_and_truncated_files(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    std::fs::write(root.join("patch.txt"), "a".repeat(600_000)).unwrap();
    git_in(root, &["add", "patch.txt"]);
    commit(root, &["commit", "-m", "base"]);
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    let show = |path: &str| {
        Op::Diff(DiffOp::Show {
            query: DiffQuery {
                thread_id: started.id.clone(),
                scope: DiffScope::Uncommitted,
                base: None,
                rev: None,
            },
            path: path.to_owned(),
        })
    };
    for (path, first, second) in [
        ("binary.bin", vec![0, 1, 2], vec![0, 1, 3]),
        ("large.txt", vec![b'x'; 1_100_000], vec![b'y'; 1_100_000]),
        ("patch.txt", vec![b'b'; 600_000], vec![b'c'; 600_000]),
    ] {
        std::fs::write(root.join(path), first).unwrap();
        let OpResult::DiffFile(before) = op(show(path), cx).await.unwrap() else {
            panic!("file")
        };
        std::fs::write(root.join(path), second).unwrap();
        let OpResult::DiffFile(after) = op(show(path), cx).await.unwrap() else {
            panic!("file")
        };
        assert_ne!(before.revision, after.revision, "{path}");
        match path {
            "binary.bin" => assert!(before.binary),
            "large.txt" => assert!(before.too_large),
            _ => assert!(before.truncated),
        }
    }
}

#[gpui_kit::test]
async fn a_text_file_view_counts_its_lines_and_carries_its_hunks_and_text(cx: &mut TestAppContext) {
    let fixture = Fixture::new(cx).await;
    let root = fixture.dir.path();
    git_in(root, &["init", "-b", "main"]);
    std::fs::write(root.join("notes.txt"), "one\ntwo\nthree\n").unwrap();
    git_in(root, &["add", "notes.txt"]);
    commit(root, &["commit", "-m", "base"]);
    let OpResult::Thread(started) = op(fixture.run(program()), cx).await.unwrap() else {
        panic!("thread")
    };
    std::fs::write(root.join("notes.txt"), "one\nTWO\nthree\nfour\n").unwrap();
    let show = Op::Diff(DiffOp::Show {
        query: DiffQuery {
            thread_id: started.id.clone(),
            scope: DiffScope::Uncommitted,
            base: None,
            rev: None,
        },
        path: "notes.txt".to_owned(),
    });
    let OpResult::DiffFile(view) = op(show, cx).await.unwrap() else {
        panic!("file")
    };
    assert_eq!((view.added, view.removed), (2, 1));
    assert!(!view.hunks.is_empty());
    assert!(!view.binary && !view.truncated && !view.too_large);
    assert_eq!(view.file.as_deref(), Some("one\nTWO\nthree\nfour\n"));
}
