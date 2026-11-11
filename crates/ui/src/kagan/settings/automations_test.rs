use kg_domain::{Automation, AutomationId, ProjectId, Timestamp};

use super::{detail, next_run};

fn automation(body: &str, next_at: Option<Timestamp>, paused: bool) -> Automation {
    Automation {
        id: AutomationId("auto_1".into()),
        project_id: ProjectId("prj_1".into()),
        name: "Morning".into(),
        body: body.into(),
        program: "claude --model fast".into(),
        minutes: Some(30),
        at: None,
        paused,
        next_at,
        threads: Vec::new(),
        failing: false,
    }
}

#[test]
fn the_row_names_the_program_and_starts_the_text_on_one_line() {
    let text = detail(&automation("first line\n\n  second   line", None, false));
    assert_eq!(text, "claude --model fast · first line second line");
    let long = detail(&automation(&"x".repeat(200), None, false));
    assert_eq!(
        long.chars().count(),
        "claude --model fast · ".chars().count() + 60
    );
}

#[test]
fn the_row_says_when_the_next_run_is_due() {
    let now = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis(),
    )
    .unwrap();
    let later = automation("ping", Some(Timestamp(now + 5 * 60_000 + 1_000)), false);
    assert_eq!(next_run(&later), "Next in 5 min");
    let due = automation("ping", Some(Timestamp(now - 1)), false);
    assert_eq!(next_run(&due), "Due now");
    let paused = automation("ping", Some(Timestamp(now + 60_000)), true);
    assert_eq!(next_run(&paused), "Paused");
    let finished = automation("ping", None, false);
    assert_eq!(next_run(&finished), "Done");
}
