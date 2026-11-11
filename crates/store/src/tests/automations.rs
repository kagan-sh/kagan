use kg_domain::{Automation, AutomationId, ProjectId, ThreadId, ThreadStatus, Timestamp};

use super::{store, thread};

fn saved(minutes: Option<u16>, at: Option<Timestamp>) -> Automation {
    Automation {
        id: AutomationId("auto_1".into()),
        project_id: ProjectId("prj_1".into()),
        name: "Morning".into(),
        body: "ping".into(),
        program: "claude".into(),
        minutes,
        at,
        paused: false,
        next_at: Some(Timestamp(5)),
        last_error: None,
        threads: Vec::new(),
        failing: false,
    }
}

#[test]
fn an_automation_keeps_its_runs_and_goes_with_its_project() {
    let store = store();
    let automation = saved(Some(15), None);
    store.insert_automation(&automation).unwrap();
    store
        .record_automation_run(
            &automation.id,
            &ThreadId("thr_1".into()),
            Timestamp(5),
            Some(Timestamp(9)),
        )
        .unwrap();
    let loaded = store.automation(&automation.id).unwrap().unwrap();
    assert_eq!(loaded.minutes, Some(15));
    assert_eq!(loaded.threads, vec![ThreadId("thr_1".into())]);
    assert_eq!(loaded.next_at, Some(Timestamp(9)));

    store
        .set_automation_paused(&automation.id, true, Timestamp(2))
        .unwrap();
    assert!(store.automation(&automation.id).unwrap().unwrap().paused);

    store
        .delete_project(&ProjectId("prj_1".into()), Timestamp(1))
        .unwrap();
    assert!(store.automations(None).unwrap().is_empty());
}

#[test]
fn only_unpaused_automations_that_are_due_are_loaded() {
    let store = store();
    let mut due = saved(Some(1), None);
    due.id = AutomationId("auto_due".into());
    due.next_at = Some(Timestamp(5));
    let mut paused = saved(Some(1), None);
    paused.id = AutomationId("auto_paused".into());
    paused.paused = true;
    paused.next_at = Some(Timestamp(5));
    let mut later = saved(Some(1), None);
    later.id = AutomationId("auto_later".into());
    later.next_at = Some(Timestamp(50));
    for automation in [&due, &paused, &later] {
        store.insert_automation(automation).unwrap();
    }
    let rows = store.due_automations(Timestamp(10)).unwrap();
    let ids: Vec<_> = rows.into_iter().map(|row| row.id).collect();
    assert_eq!(ids, vec![due.id]);
}

// A run counts as failed when its thread ended in an error. Three failed runs in a row are a
// failing automation; a run that did not fail breaks the row, and a resume starts a new streak.
#[test]
fn the_failure_streak_is_the_last_three_runs_since_the_last_resume() {
    let store = store();
    let automation = saved(Some(15), None);
    store.insert_automation(&automation).unwrap();
    let run = |n: i64, status: ThreadStatus| {
        let id = format!("thr_{n}");
        store.put_thread(&thread(&id, n)).unwrap();
        store
            .set_thread_status(&ThreadId(id.clone()), status, Timestamp(n))
            .unwrap();
        store
            .record_automation_run(&automation.id, &ThreadId(id), Timestamp(n), None)
            .unwrap();
        store.automation(&automation.id).unwrap().unwrap().failing
    };
    assert!(!run(1, ThreadStatus::Error));
    assert!(!run(2, ThreadStatus::Error));
    assert!(
        !run(3, ThreadStatus::Idle),
        "a run that did not fail breaks the streak"
    );
    assert!(!run(4, ThreadStatus::Error));
    assert!(!run(5, ThreadStatus::Error));
    assert!(
        run(6, ThreadStatus::Error),
        "the last three runs all failed"
    );
    store
        .set_automation_paused(&automation.id, false, Timestamp(7))
        .unwrap();
    assert!(
        !store.automation(&automation.id).unwrap().unwrap().failing,
        "a resume starts a new streak"
    );
}

#[test]
fn a_failed_start_is_kept_until_a_run_starts() {
    let store = store();
    let automation = saved(Some(15), None);
    store.insert_automation(&automation).unwrap();
    store
        .postpone_automation(&automation.id, Timestamp(9), "no such program")
        .unwrap();
    let failed = store.automation(&automation.id).unwrap().unwrap();
    assert_eq!(failed.last_error.as_deref(), Some("no such program"));
    assert_eq!(failed.next_at, Some(Timestamp(9)));

    store
        .record_automation_run(
            &automation.id,
            &ThreadId("thr_1".into()),
            Timestamp(10),
            Some(Timestamp(20)),
        )
        .unwrap();
    assert!(
        store
            .automation(&automation.id)
            .unwrap()
            .unwrap()
            .last_error
            .is_none()
    );
}
