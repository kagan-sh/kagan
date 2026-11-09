use super::*;

#[test]
fn an_ended_program_is_seen_without_reaping_it() {
    let mut ended_soon = std::process::Command::new("true").spawn().unwrap();
    let pid = ended_soon.id();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while exit(pid).is_none() {
        assert!(std::time::Instant::now() < deadline, "never seen as ended");
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(exit(pid).is_some(), "looking reaped it");
    assert!(ended_soon.try_wait().unwrap().is_some());
}

#[test]
fn a_running_program_has_not_ended() {
    let mut running = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    assert!(exit(running.id()).is_none());
    running.kill().unwrap();
    running.wait().unwrap();
}

#[test]
fn pids_that_are_not_ours_have_not_ended() {
    assert!(exit(0).is_none());
    assert!(exit(1).is_none());
}
