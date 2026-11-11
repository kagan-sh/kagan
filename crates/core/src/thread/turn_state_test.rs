use std::time::{Duration, Instant};

use super::{Change, HOOK_SILENCE, QUIET_OUTPUT, SUBMIT_WINDOW, Signal, Source, TurnState};

fn at(base: Instant, secs: f64) -> Instant {
    base + Duration::from_secs_f64(secs)
}

fn run(source: Source, steps: &[(Signal, f64)]) -> (TurnState, Vec<Option<Change>>) {
    let base = Instant::now();
    let mut state = TurnState::new(source);
    let changes = steps
        .iter()
        .map(|(signal, secs)| state.apply(*signal, at(base, *secs)))
        .collect();
    (state, changes)
}

#[test]
fn a_hook_start_opens_and_its_end_closes() {
    let (state, changes) = run(
        Source::Hook,
        &[(Signal::HookStart, 0.0), (Signal::HookEnd, 1.0)],
    );
    assert_eq!(changes, [Some(Change::Opened), Some(Change::Closed)]);
    assert!(!state.open());
}

#[test]
fn a_start_while_open_and_an_end_while_idle_do_nothing() {
    let (_, changes) = run(
        Source::Hook,
        &[
            (Signal::HookEnd, 0.0),
            (Signal::HookStart, 1.0),
            (Signal::HookStart, 2.0),
            (Signal::HookEnd, 3.0),
            (Signal::HookEnd, 4.0),
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
        &[
            (Signal::HookStart, 0.0),
            (Signal::HookEnd, 1.0),
            (Signal::Ended, 2.0),
        ],
    );
    assert_eq!(changes[2], None);
    assert!(!state.open());
}

#[test]
fn a_title_never_opens_a_hook_run() {
    let (state, changes) = run(Source::Hook, &[(Signal::Title { busy: true }, 0.0)]);
    assert_eq!(changes, [None]);
    assert!(!state.open());
}

#[test]
fn a_hook_run_does_not_close_on_a_plain_title_without_a_spinner_in_that_turn() {
    let (state, changes) = run(
        Source::Hook,
        &[
            (Signal::HookStart, 0.0),
            (Signal::Title { busy: false }, 5.0),
            (Signal::Title { busy: false }, 60.0),
        ],
    );
    assert_eq!(changes, [Some(Change::Opened), None, None]);
    assert!(state.open());
}

#[test]
fn a_stale_plain_clock_from_an_earlier_turn_does_not_close_the_next_one() {
    let (state, changes) = run(
        Source::Hook,
        &[
            (Signal::HookStart, 0.0),
            (Signal::Title { busy: true }, 1.0),
            (Signal::Title { busy: false }, 1.5),
            (Signal::HookEnd, 2.0),
            (Signal::HookStart, 62.0),
            (Signal::Title { busy: false }, 62.0),
        ],
    );
    assert_eq!(changes[4], Some(Change::Opened));
    assert_eq!(changes[5], None, "the second turn closed on its first tick");
    assert!(state.open());
}

#[test]
fn a_hook_run_closes_once_its_spinner_has_been_plain_for_the_grace() {
    let (state, changes) = run(
        Source::Hook,
        &[
            (Signal::HookStart, 0.0),
            (Signal::Title { busy: true }, 1.0),
            (Signal::Title { busy: false }, 2.0),
            (Signal::Title { busy: false }, 3.0),
        ],
    );
    assert_eq!(changes[3], Some(Change::Closed));
    assert!(!state.open());
}

#[test]
fn an_undecided_run_opens_on_the_spinner_and_keeps_title_as_its_source() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Title { busy: true }, 0.0),
            (Signal::Title { busy: false }, 2.0),
            (Signal::Title { busy: false }, 3.0),
        ],
    );
    assert_eq!(changes, [Some(Change::Opened), None, Some(Change::Closed)]);
    assert_eq!(state.source(), Source::Title);
}

#[test]
fn a_submit_then_output_opens_a_quiet_turn() {
    let (state, changes) = run(
        Source::Undecided,
        &[(Signal::Submit { screen: 0 }, 0.0), (Signal::Output, 1.0)],
    );
    assert_eq!(changes, [None, Some(Change::Opened)]);
    assert_eq!(state.source(), Source::Quiet);
}

#[test]
fn a_submit_with_no_output_opens_nothing() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Tick, 1.0),
            (Signal::Output, 4.0),
        ],
    );
    assert_eq!(changes, [None, None, None]);
    assert!(!state.open());
}

#[test]
fn an_answer_after_the_submit_window_opens_nothing() {
    let late = SUBMIT_WINDOW.as_secs_f64() + 1.0;
    let (state, changes) = run(
        Source::Quiet,
        &[(Signal::Submit { screen: 0 }, 0.0), (Signal::Output, late)],
    );
    assert_eq!(changes, [None, None]);
    assert!(!state.open());
}

#[test]
fn a_submit_while_open_or_on_a_hook_run_does_nothing() {
    let (_, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Output, 0.5),
            (Signal::Submit { screen: 0 }, 1.0),
        ],
    );
    assert_eq!(changes, [None, Some(Change::Opened), None]);
    let (_, changes) = run(
        Source::Hook,
        &[(Signal::Submit { screen: 0 }, 0.0), (Signal::Output, 0.5)],
    );
    assert_eq!(changes, [None, None]);
}

#[test]
fn a_spinner_takes_over_an_open_quiet_turn_without_a_new_event() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Output, 0.5),
            (Signal::Title { busy: true }, 1.0),
        ],
    );
    assert_eq!(changes, [None, Some(Change::Opened), None]);
    assert_eq!(state.source(), Source::Title);
}

#[test]
fn a_hook_takes_over_an_open_quiet_turn_without_a_new_event() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Output, 0.5),
            (Signal::HookStart, 1.0),
        ],
    );
    assert_eq!(changes, [None, Some(Change::Opened), None]);
    assert_eq!(state.source(), Source::Hook);
}

#[test]
fn quiet_output_closes_a_quiet_turn_only_after_its_limit() {
    let just_under = QUIET_OUTPUT.as_secs_f64() - 0.001;
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Output, 0.5),
            (Signal::Tick, 0.5 + just_under),
            (Signal::Tick, 0.5 + QUIET_OUTPUT.as_secs_f64()),
        ],
    );
    assert_eq!(
        changes,
        [None, Some(Change::Opened), None, Some(Change::Closed)]
    );
    assert!(!state.open());
}

#[test]
fn a_screen_that_only_repaints_the_same_text_does_not_keep_a_quiet_turn_open() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Screen(7), 0.5),
            (Signal::Screen(7), 10.0),
            (Signal::Screen(7), 20.0),
            (Signal::Tick, 20.5),
        ],
    );
    assert_eq!(changes[1], Some(Change::Opened));
    assert_eq!(changes[2], None);
    assert_eq!(changes[3], None);
    assert_eq!(
        changes[4],
        Some(Change::Closed),
        "identical repaints kept the turn open"
    );
    assert!(!state.open());
}

#[test]
fn a_screen_with_new_text_keeps_a_quiet_turn_open() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 0 }, 0.0),
            (Signal::Screen(7), 0.5),
            (Signal::Screen(8), 10.0),
            (Signal::Tick, 20.0),
        ],
    );
    assert_eq!(changes[2], None);
    assert_eq!(changes[3], None);
    assert!(state.open());
}

#[test]
fn text_is_wanted_only_while_a_quiet_turn_is_open_or_a_submit_waits() {
    let base = Instant::now();
    let mut state = TurnState::new(Source::Undecided);
    assert!(!state.wants_text());
    state.apply(Signal::Submit { screen: 0 }, base);
    assert!(state.wants_text());
    state.apply(Signal::Output, base);
    assert!(state.wants_text());
    state.apply(Signal::Ended, base);
    assert!(!state.wants_text());
}

#[test]
fn silence_never_closes_a_spinner_or_an_undecided_turn() {
    let (state, changes) = run(
        Source::Title,
        &[
            (Signal::Title { busy: true }, 0.0),
            (Signal::Tick, HOOK_SILENCE.as_secs_f64() * 10.0),
        ],
    );
    assert_eq!(changes, [Some(Change::Opened), None]);
    assert!(state.open());
}

#[test]
fn the_backstop_closes_a_silent_hook_turn_but_not_early() {
    let just_under = HOOK_SILENCE.as_secs_f64() - 0.001;
    let (state, changes) = run(
        Source::Hook,
        &[
            (Signal::HookStart, 0.0),
            (Signal::Tick, just_under),
            (Signal::Tick, HOOK_SILENCE.as_secs_f64()),
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
        let (state, changes) = run(source, &[(opening, 0.0), (Signal::Ended, 1.0)]);
        assert_eq!(changes[1], Some(Change::Closed), "{source:?}");
        assert!(!state.open(), "{source:?}");
    }
}

#[test]
fn a_hook_end_closes_a_turn_the_title_opened() {
    let (state, changes) = run(
        Source::Undecided,
        &[(Signal::Title { busy: true }, 0.0), (Signal::HookEnd, 1.0)],
    );
    assert_eq!(changes[1], Some(Change::Closed));
    assert!(!state.open());
}

#[test]
fn a_redraw_right_after_a_submit_is_not_output_when_the_text_did_not_change() {
    let (state, changes) = run(
        Source::Undecided,
        &[
            (Signal::Submit { screen: 7 }, 0.0),
            (Signal::Screen(7), 0.5),
        ],
    );
    assert_eq!(changes, [None, None], "an unchanged screen opened a turn");
    assert!(!state.open());
}
