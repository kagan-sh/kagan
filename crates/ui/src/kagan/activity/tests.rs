use super::*;

fn strip() -> Strip {
    Strip::new()
}

#[gpui_kit::test]
fn the_log_keeps_the_last_hundred_lines() {
    let mut strip = strip();
    for line in 0..120 {
        strip.say(line.to_string());
    }
    assert_eq!(strip.kept(), KEEP);
    assert_eq!(
        strip.lines.front().map(|(_, line)| line.as_ref()),
        Some("20")
    );
}

#[gpui_kit::test]
fn the_log_shows_the_last_seven_oldest_first() {
    let mut strip = strip();
    for line in 1..=10 {
        strip.say(line.to_string());
    }
    let lines: Vec<String> = strip
        .tail()
        .into_iter()
        .map(|(_, line)| line.to_string())
        .collect();
    assert_eq!(lines, ["4", "5", "6", "7", "8", "9", "10"]);
}

#[gpui_kit::test]
fn a_repeated_update_or_install_line_is_written_once() {
    let mut strip = strip();
    let ready = UpdateState::Ready {
        version: "2.0.0".into(),
        notes: String::new(),
        at_quit: false,
    };
    strip.update_state(&ready);
    strip.update_state(&ready);
    strip.installing(Some("demo"));
    strip.installing(Some("demo"));
    strip.installing(None);
    assert_eq!(strip.kept(), 2);
}
