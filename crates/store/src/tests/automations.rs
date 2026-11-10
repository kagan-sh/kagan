use kg_domain::{Automation, AutomationId, ProjectId, ThreadId, Timestamp};

use super::store;

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
        threads: Vec::new(),
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

    store.set_automation_paused(&automation.id, true).unwrap();
    assert!(store.automation(&automation.id).unwrap().unwrap().paused);

    store
        .delete_project(&ProjectId("prj_1".into()), Timestamp(1))
        .unwrap();
    assert!(store.automations(None).unwrap().is_empty());
}
