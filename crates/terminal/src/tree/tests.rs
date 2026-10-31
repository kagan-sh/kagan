use super::*;

#[test]
fn pids_that_are_not_ours_are_refused() {
    assert!(pid_of(0).is_none());
    assert!(pid_of(1).is_none());
    assert!(pid_of(u32::MAX).is_none());
    assert!(pid_of(2).is_some());
}

#[test]
fn an_ended_program_is_seen_without_reaping_it() {
    let mut ended_soon = std::process::Command::new("true").spawn().unwrap();
    let pid = ended_soon.id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !ended(pid) {
        assert!(std::time::Instant::now() < deadline, "never seen as ended");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(ended(pid), "looking reaped it");
    assert!(ended_soon.try_wait().unwrap().is_some());
}

#[test]
fn a_running_program_has_not_ended() {
    let mut running = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    assert!(!ended(running.id()));
    running.kill().unwrap();
    running.wait().unwrap();
}

#[test]
fn pids_that_are_not_ours_have_not_ended() {
    assert!(!ended(0));
    assert!(!ended(1));
}
