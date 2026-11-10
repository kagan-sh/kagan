use std::collections::BTreeMap;

use kg_domain::{EnvOp, OpResult};

use super::{apply, merge_scopes};

#[test]
fn windows_names_fold_so_the_project_value_wins() {
    let global = BTreeMap::from([("foo".to_owned(), "global".to_owned())]);
    let own = BTreeMap::from([("FOO".to_owned(), "project".to_owned())]);
    let folded = merge_scopes(global.clone(), &own, true);
    assert_eq!(folded.len(), 1, "{folded:?}");
    assert_eq!(folded.get("FOO").map(String::as_str), Some("project"));
    let exact = merge_scopes(global, &own, false);
    assert_eq!(exact.len(), 2, "{exact:?}");
}

#[test]
fn concurrent_sets_keep_every_name() {
    let dir = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        for worker in 0..8 {
            let dir = dir.path();
            scope.spawn(move || {
                for number in 0..25 {
                    apply(
                        dir,
                        EnvOp::Set {
                            name: format!("W{worker}_{number}"),
                            project_id: None,
                            value: Some("value".into()),
                            thread_id: None,
                        },
                    )
                    .unwrap();
                }
            });
        }
    });
    let OpResult::Variables(names) = apply(dir.path(), EnvOp::List { project_id: None }).unwrap()
    else {
        panic!("variables")
    };
    assert_eq!(names.len(), 200, "lost writes: {}", 200 - names.len());
}
