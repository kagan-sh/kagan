use std::time::Duration;

use super::{Change, HOOK_SILENCE, QUIET_OUTPUT, Signal, Source, TurnState};

fn run(source: Source, signals: &[Signal]) -> (TurnState, Vec<Option<Change>>) {
    let mut state = TurnState::new(source);
    let changes = signals.iter().map(|signal| state.apply(*signal)).collect();
    (state, changes)
}

#[test]
fn a_hook_start_opens_and_its_end_closes() {
    let (state, changes) = run(Source::Hook, &[Signal::HookStart, Signal::HookEnd]);
    assert_eq!(changes, [Some(Change::Opened), Some(Change::Closed)]);
    assert!(!state.open());
}

#[test]
fn a_start_while_open_and_an_end_while_idle_do_nothing() {
    let (_, changes) = run(
        Source::Hook,
        &[
            Signal::HookEnd,
            Signal::HookStart,
            Signal::HookStart,
            Signal::HookEnd,
            Signal::HookEnd,
        ],
    );
    assert_eq!(
        changes,
        [None, Some(Change::Opened), None, Some(Change::Closed), None]
    );
}

#[test]
fn a_late_stop_after_the_close_is_harmless() {
    let (state, changes) = run(
        Source::Hook,
        &[Signal::HookStart, Signal::HookEnd, Signal::Ended],
    );
    assert_eq!(changes[2], None);
    assert!(!state.open());
}

#[test]
fn a_title_never_opens_a_hook_run() {
    let (state, changes) = run(Source::Hook, &[Signal::Title { busy: true }]);
    assert_eq!(changes, [None]);
    assert!(!state.open());
}

#[test]
fn a_hook_run_title_going_plain_closes_its_open_turn() {
    let (state, changes) = run(
        Source::Hook,
        &[Signal::HookStart, Signal::Title { busy: false }],
    );
    assert_eq!(changes[1], Some(Change::Closed));
    assert!(!state.open());
}

#[test]
fn an_undecided_run_opens_on_the_spinner_and_keeps_title_as_its_source() {
    let (state, changes) = run(
        Source::Undecided,
        &[Signal::Title { busy: true }, Signal::Title { busy: false }],
    );
    assert_eq!(changes, [Some(Change::Opened), Some(Change::Closed)]);
    assert_eq!(state.source(), Source::Title);
}

#[test]
fn a_submit_opens_a_quiet_turn_when_nothing_else_owns_the_run() {
    let (state, changes) = run(Source::Undecided, &[Signal::Submit]);
    assert_eq!(changes, [Some(Change::Opened)]);
    assert_eq!(state.source(), Source::Quiet);
}

#[test]
fn a_submit_while_open_or_on_a_hook_run_does_nothing() {
    let (_, changes) = run(Source::Undecided, &[Signal::Submit, Signal::Submit]);
    assert_eq!(changes, [Some(Change::Opened), None]);
    let (_, changes) = run(Source::Hook, &[Signal::Submit]);
    assert_eq!(changes, [None]);
}

#[test]
fn a_spinner_takes_over_an_open_quiet_turn_without_a_new_event() {
    let (state, changes) = run(
        Source::Undecided,
        &[Signal::Submit, Signal::Title { busy: true }],
    );
    assert_eq!(changes, [Some(Change::Opened), None]);
    assert_eq!(state.source(), Source::Title);
}

#[test]
fn a_hook_takes_over_an_open_quiet_turn_without_a_new_event() {
    let (state, changes) = run(Source::Undecided, &[Signal::Submit, Signal::HookStart]);
    assert_eq!(changes, [Some(Change::Opened), None]);
    assert_eq!(state.source(), Source::Hook);
}

#[test]
fn quiet_output_closes_a_quiet_turn_only_after_its_limit() {
    let just_under = QUIET_OUTPUT.checked_sub(Duration::from_millis(1)).unwrap();
    let (state, changes) = run(
        Source::Undecided,
        &[
            Signal::Submit,
            Signal::Silence(just_under),
            Signal::Silence(QUIET_OUTPUT),
        ],
    );
    assert_eq!(changes, [Some(Change::Opened), None, Some(Change::Closed)]);
    assert!(!state.open());
}

#[test]
fn silence_never_closes_a_spinner_or_an_undecided_turn() {
    let (state, changes) = run(
        Source::Title,
        &[
            Signal::Title { busy: true },
            Signal::Silence(HOOK_SILENCE * 10),
        ],
    );
    assert_eq!(changes, [Some(Change::Opened), None]);
    assert!(state.open());
}

#[test]
fn the_backstop_closes_a_silent_hook_turn_but_not_early() {
    let just_under = HOOK_SILENCE.checked_sub(Duration::from_millis(1)).unwrap();
    let (state, changes) = run(
        Source::Hook,
        &[
            Signal::HookStart,
            Signal::Silence(just_under),
            Signal::Silence(HOOK_SILENCE),
        ],
    );
    assert_eq!(changes, [Some(Change::Opened), None, Some(Change::Closed)]);
    assert!(!state.open());
}

#[test]
fn an_exit_closes_from_any_source() {
    for source in [
        Source::Undecided,
        Source::Hook,
        Source::Title,
        Source::Quiet,
    ] {
        let opening = match source {
            Source::Hook => Signal::HookStart,
            _ => Signal::Title { busy: true },
        };
        let (state, changes) = run(source, &[opening, Signal::Ended]);
        assert_eq!(changes[1], Some(Change::Closed), "{source:?}");
        assert!(!state.open(), "{source:?}");
    }
}

#[test]
fn a_hook_end_closes_a_turn_the_title_opened() {
    let (state, changes) = run(
        Source::Undecided,
        &[Signal::Title { busy: true }, Signal::HookEnd],
    );
    assert_eq!(changes[1], Some(Change::Closed));
    assert!(!state.open());
}
