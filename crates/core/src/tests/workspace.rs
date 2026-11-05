use kg_domain::*;

#[test]
fn a_data_dir_that_cannot_be_made_is_named() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("not-a-folder");
    std::fs::write(&file, "x").unwrap();
    let data_dir = file.join("kagan");
    let Err(error) = crate::workspace::open_data(&data_dir) else {
        panic!("opened");
    };
    let message = error.to_string();
    assert!(
        message.contains(&data_dir.display().to_string()),
        "{message}"
    );
}

#[test]
fn a_stop_waits_in_the_queue_and_a_run_does_not() {
    assert!(crate::dispatch::serialized(&Op::Thread(ThreadOp::Stop {
        id: ThreadId("thr_a".into()),
    })));
    assert!(!crate::dispatch::serialized(&Op::Thread(ThreadOp::Run(
        Run {
            project_id: ProjectId("project".into()),
            program: "sh".into(),
            title: None,
            environment: EnvironmentSpec::Worktree {
                base_branch: None,
                branch: None,
            },
            visibility: None,
            parent_thread_id: None,
            lifecycle_owner_thread_id: None,
            metadata: None,
        },
    ))));
}
