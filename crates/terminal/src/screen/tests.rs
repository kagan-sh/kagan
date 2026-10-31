use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::index::{Column, Line};

use crate::{Motion, Mouse};

use super::*;

fn screen(text: &str) -> Screen {
    let mut screen = Screen::new(12, 4);
    screen.feed(text.as_bytes());
    screen
}

fn replay(screen: &Screen) -> Screen {
    let mut copy = Screen::new(
        u16::try_from(screen.term.columns()).unwrap(),
        u16::try_from(screen.term.screen_lines()).unwrap(),
    );
    assert!(copy.feed(screen.draw().as_bytes()).is_empty());
    copy
}

fn cells(screen: &Screen) -> Vec<(char, Color, Color, Flags)> {
    let style = ATTRIBUTES
        .iter()
        .fold(Flags::empty(), |all, (flag, _)| all | *flag);
    let grid = screen.term.grid();
    (0..)
        .map(Line)
        .take(grid.screen_lines())
        .flat_map(|line| (0..grid.columns()).map(move |column| (line, Column(column))))
        .map(|(line, column)| &grid[line][column])
        .map(|cell| (cell.c, cell.fg, cell.bg, cell.flags & style))
        .collect()
}

fn same_after_replay(text: &str) {
    let screen = screen(text);
    assert_eq!(cells(&screen), cells(&replay(&screen)), "{text:?}");
}

#[test]
fn a_frame_redraws_the_text_on_its_own_rows() {
    let screen = screen("one\r\ntwo");
    let text: Vec<char> = cells(&screen).iter().map(|cell| cell.0).collect();
    assert_eq!(text[..3], ['o', 'n', 'e']);
    assert_eq!(text[12..15], ['t', 'w', 'o']);
    same_after_replay("one\r\ntwo\r\n\r\nfour");
}

#[test]
fn a_frame_draws_without_scrolling() {
    let frame = screen("one\r\ntwo").draw();
    assert!(!frame.contains('\n'));
    assert!(frame.contains("\x1b[1;1H") && frame.contains("\x1b[4;1H"));
}

#[test]
fn a_frame_keeps_every_color() {
    for code in [
        "31",
        "91",
        "41",
        "101",
        "38;5;200",
        "48;5;17",
        "38;2;1;2;3",
        "48;2;4;5;6",
        "36;45",
    ] {
        same_after_replay(&format!("\x1b[{code}mx\x1b[0my"));
    }
}

#[test]
fn a_frame_keeps_every_attribute() {
    for code in ["1", "2", "3", "4", "7", "8", "9", "1;3;4;7"] {
        let screen = screen(&format!("\x1b[{code}mx"));
        let first = cells(&screen)[0];
        assert!(!first.3.is_empty(), "{code}");
        assert_eq!(cells(&screen), cells(&replay(&screen)), "{code}");
    }
}

#[test]
fn every_kind_of_underline_is_drawn_as_one() {
    let screen = screen("\x1b[4:4mx\x1b[4:3my");
    let drawn = replay(&screen);
    let flags: Vec<Flags> = cells(&drawn)[..2].iter().map(|cell| cell.3).collect();
    assert_eq!(
        flags,
        [Flags::UNDERLINE, Flags::UNDERLINE],
        "{:?}",
        cells(&screen)[..2]
            .iter()
            .map(|cell| cell.3)
            .collect::<Vec<_>>()
    );
}

#[test]
fn a_frame_keeps_wide_and_combining_characters() {
    same_after_replay("日本e\u{301}x");
    let frame = screen("e\u{301}").draw();
    assert!(frame.contains("e\u{301}"));
}

#[test]
fn a_frame_keeps_the_cursor() {
    let screen = screen("\x1b[3;5H");
    assert_eq!(
        replay(&screen).term.grid().cursor.point,
        screen.term.grid().cursor.point
    );
    assert!(screen.draw().ends_with("\x1b[3;5H\x1b[?25h"));
    let hidden = self::screen("\x1b[2;2H\x1b[?25l");
    assert!(hidden.draw().ends_with("\x1b[2;2H"));
    assert!(!replay(&hidden).term.mode().contains(TermMode::SHOW_CURSOR));
}

#[test]
fn a_frame_keeps_the_modes_that_change_what_keys_send() {
    for (flag, code) in MODES {
        let on = screen(&format!("\x1b[?{code}h"));
        assert!(on.term.mode().contains(flag), "{code}");
        assert!(replay(&on).term.mode().contains(flag), "{code}");
        let mut off = replay(&on);
        off.feed(format!("\x1b[?{code}l").as_bytes());
        assert!(off.draw().contains(&format!("\x1b[?{code}l")), "{code}");
        let mut turned_off = replay(&on);
        assert!(turned_off.feed(screen("").draw().as_bytes()).is_empty());
        assert!(!turned_off.term.mode().contains(flag), "{code}");
    }
}

#[test]
fn a_frame_keeps_the_keypad_mode() {
    let on = screen("\x1b=");
    assert!(on.term.mode().contains(TermMode::APP_KEYPAD));
    assert!(replay(&on).term.mode().contains(TermMode::APP_KEYPAD));
    let mut off = replay(&on);
    off.feed(screen("").draw().as_bytes());
    assert!(!off.term.mode().contains(TermMode::APP_KEYPAD));
}

#[test]
fn a_resized_screen_draws_its_new_size() {
    let mut screen = screen("one");
    screen.apply(Edit::Resize(20, 6));
    assert_eq!(cells(&screen).len(), 20 * 6);
    assert!(screen.draw().contains("\x1b[6;1H"));
    assert_eq!(cells(&screen), cells(&replay(&screen)));
}

#[test]
fn the_screen_answers_where_the_cursor_is() {
    let mut screen = Screen::new(12, 4);
    assert_eq!(screen.feed(b"\x1b[6n"), b"\x1b[1;1R");
    assert_eq!(screen.feed(b"ab\x1b[6n"), b"\x1b[1;3R");
    assert_eq!(screen.feed(b"\r\n\x1b[6n"), b"\x1b[2;1R");
}

#[test]
fn a_query_split_across_reads_is_answered_once() {
    let mut screen = Screen::new(12, 4);
    assert!(screen.feed(b"\x1b[").is_empty());
    assert!(screen.feed(b"6").is_empty());
    assert_eq!(screen.feed(b"n"), b"\x1b[1;1R");
    assert!(screen.feed(b"x").is_empty());
}

#[test]
fn every_query_in_one_read_is_answered() {
    let mut screen = Screen::new(12, 4);
    assert_eq!(
        screen.feed(b"\x1b[6n\x1b[6n\x1b[5n"),
        b"\x1b[1;1R\x1b[1;1R\x1b[0n"
    );
}

#[test]
fn the_screen_answers_device_and_color_queries() {
    let mut screen = Screen::new(12, 4);
    assert_eq!(screen.feed(b"\x1b[c"), b"\x1b[?6c");
    assert_eq!(
        screen.feed(b"\x1b]10;?\x07"),
        b"\x1b]10;rgb:ffff/ffff/ffff\x07"
    );
    assert_eq!(
        screen.feed(b"\x1b]11;?\x07"),
        b"\x1b]11;rgb:0b0b/0a0a/0909\x07"
    );
    assert_eq!(
        screen.feed(b"\x1b]12;?\x07"),
        b"\x1b]12;rgb:ffff/ffff/ffff\x07"
    );
}

#[test]
fn requests_the_screen_cannot_know_get_no_reply() {
    let mut screen = Screen::new(12, 4);
    assert!(screen.feed(b"\x1b]4;1;?\x07").is_empty());
    assert!(screen.feed(b"\x1b[14t").is_empty());
    assert!(screen.feed(b"\x1b]0;a title\x07\x07").is_empty());
}

#[test]
fn every_kind_of_color_has_its_code() {
    let named = |name| Color::Named(name);
    for (color_name, fg, bg) in [
        (NamedColor::Black, "30", "40"),
        (NamedColor::White, "37", "47"),
        (NamedColor::BrightBlack, "90", "100"),
        (NamedColor::BrightWhite, "97", "107"),
        (NamedColor::DimBlack, "30", "40"),
        (NamedColor::DimWhite, "37", "47"),
        (NamedColor::Foreground, "39", "49"),
        (NamedColor::Background, "39", "49"),
        (NamedColor::BrightForeground, "39", "49"),
    ] {
        assert_eq!(color(named(color_name), false), fg, "{color_name:?}");
        assert_eq!(color(named(color_name), true), bg, "{color_name:?}");
    }
    assert_eq!(color(Color::Indexed(200), false), "38;5;200");
    assert_eq!(color(Color::Indexed(200), true), "48;5;200");
    assert_eq!(
        color(Color::Spec(Rgb { r: 1, g: 2, b: 3 }), false),
        "38;2;1;2;3"
    );
    assert_eq!(
        color(Color::Spec(Rgb { r: 1, g: 2, b: 3 }), true),
        "48;2;1;2;3"
    );
}

#[test]
fn a_hold_the_program_never_releases_ends_after_its_timeout() {
    let mut screen = Screen::new(12, 4);
    assert!(screen.feed(b"\x1b[?2026hheld\x1b[6n").is_empty());
    assert!(!cells(&screen).iter().any(|cell| cell.0 == 'h'));
    assert_eq!(screen.expire_hold(), None);
    std::thread::sleep(std::time::Duration::from_millis(300));
    assert_eq!(screen.expire_hold(), Some(b"\x1b[1;5R".to_vec()));
    assert_eq!(
        cells(&screen)[..4]
            .iter()
            .map(|cell| cell.0)
            .collect::<String>(),
        "held"
    );
    assert_eq!(screen.expire_hold(), None);
}

#[test]
fn a_hold_the_program_releases_needs_no_timeout() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"\x1b[?2026hheld\x1b[?2026l");
    assert_eq!(cells(&screen)[0].0, 'h');
    assert_eq!(screen.expire_hold(), None);
}

#[test]
fn a_frame_erases_what_the_client_showed_before() {
    assert!(screen("x").draw().starts_with("\x1b[?25l\x1b[0m\x1b[2J"));
    let shown = screen("one\r\ntwo");
    let mut client = Screen::new(12, 4);
    client.feed(b"\x1b[1;1Hjunk junk junk\x1b[3;1Hjunk");
    client.feed(shown.draw().as_bytes());
    assert_eq!(cells(&shown), cells(&client));
}

#[test]
fn what_follows_a_symbol_is_placed_in_case_the_client_counts_it_differently() {
    let drawn = screen("\u{e9}\u{65e5}x").draw();
    assert!(drawn.contains("\u{e9}\x1b[1;2H"), "{drawn:?}");
    assert!(drawn.contains("\u{65e5}\x1b[1;4H"), "{drawn:?}");
    assert!(!screen("abc").draw().contains("a\x1b[1;"));
}

#[test]
fn a_title_is_kept_and_a_pop_that_restores_no_title_clears_it() {
    let mut screen = Screen::new(12, 4);
    assert_eq!(screen.title(), None);
    // Pushed before any title was set, so popping it restores none.
    screen.feed(b"\x1b[22;0t");
    screen.feed(b"\x1b]2;work\x07");
    assert_eq!(screen.title(), Some("work"));
    screen.feed(b"\x1b[23;0t");
    assert_eq!(screen.title(), None);
}

#[test]
fn a_full_reset_keeps_the_stored_title() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"\x1b]2;work\x07x");
    assert_eq!(screen.title(), Some("work"));
    assert_eq!(rows(&screen)[0], "x");
    screen.feed(b"\x1bc");
    assert_eq!(screen.title(), Some("work"));
    assert_eq!(rows(&screen)[0], "");
}

fn rows(screen: &Screen) -> Vec<String> {
    screen
        .grid()
        .lines
        .iter()
        .map(|line| line.iter().map(|cell| cell.c).collect::<String>())
        .map(|text| text.trim_end().to_owned())
        .collect()
}

/// A 4-row screen after `lines` lines, `line 1` first.
fn printed(lines: usize) -> Screen {
    let mut screen = Screen::new(12, 4);
    for n in 1..=lines {
        screen.feed(format!("line {n}\r\n").as_bytes());
    }
    screen
}

#[test]
fn the_history_keeps_lines_past_the_screen() {
    let mut screen = printed(10);
    assert_eq!(rows(&screen), ["line 8", "line 9", "line 10", ""]);
    screen.apply(Edit::Scroll(3));
    assert_eq!(rows(&screen), ["line 5", "line 6", "line 7", "line 8"]);
    screen.apply(Edit::Scroll(-10));
    assert_eq!(rows(&screen), ["line 8", "line 9", "line 10", ""]);
}

#[test]
fn a_scrolled_view_holds_while_output_arrives_and_typing_goes_back_to_the_bottom() {
    let mut screen = printed(10);
    screen.apply(Edit::Scroll(3));
    let held = rows(&screen);
    screen.feed(b"line 11\r\n");
    assert_eq!(rows(&screen), held);
    screen.apply(Edit::Bottom);
    assert_eq!(rows(&screen), ["line 9", "line 10", "line 11", ""]);
}

#[test]
fn the_transcript_is_the_history_and_the_screen() {
    let screen = printed(10);
    let text = screen.transcript();
    assert!(text.starts_with("line 1\nline 2\n"), "{text:?}");
    assert!(text.ends_with("line 9\nline 10\n"), "{text:?}");
}

#[test]
fn a_selection_word_copies_its_text_and_a_line_copies_the_line() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"one two\r\nthree");
    screen.apply(Edit::Select {
        mode: SelectMode::Word,
        column: 5,
        row: 0,
        right: false,
    });
    assert_eq!(screen.selection_text().as_deref(), Some("two"));
    screen.apply(Edit::Select {
        mode: SelectMode::Line,
        column: 0,
        row: 1,
        right: false,
    });
    assert_eq!(screen.selection_text().as_deref(), Some("three"));
}

#[test]
fn a_line_selection_keeps_the_blank_lines_it_covers() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"one\r\n\r\n\r\nthree");
    screen.apply(Edit::Select {
        mode: SelectMode::Line,
        column: 0,
        row: 0,
        right: false,
    });
    screen.apply(Edit::Select {
        mode: SelectMode::Extend,
        column: 0,
        row: 2,
        right: false,
    });
    assert_eq!(screen.selection_text().as_deref(), Some("one\n\n"));
}

#[test]
fn a_selection_across_a_wrapped_line_copies_without_a_newline() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"abcdefghijklmnop");
    screen.apply(Edit::Select {
        mode: SelectMode::Simple,
        column: 0,
        row: 0,
        right: false,
    });
    screen.apply(Edit::Select {
        mode: SelectMode::Extend,
        column: 3,
        row: 1,
        right: true,
    });
    assert_eq!(screen.selection_text().as_deref(), Some("abcdefghijklmnop"));
}

#[test]
fn a_block_selection_copies_its_rectangle() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"abcd\r\nefgh");
    screen.apply(Edit::Select {
        mode: SelectMode::Block,
        column: 1,
        row: 0,
        right: false,
    });
    screen.apply(Edit::Select {
        mode: SelectMode::Extend,
        column: 2,
        row: 1,
        right: true,
    });
    assert_eq!(screen.selection_text().as_deref(), Some("bc\nfg"));
}

#[test]
fn a_selection_in_a_scrolled_view_is_in_the_history() {
    let mut screen = printed(10);
    screen.apply(Edit::Scroll(3));
    screen.apply(Edit::Select {
        mode: SelectMode::Line,
        column: 0,
        row: 0,
        right: false,
    });
    assert_eq!(screen.selection_text().as_deref(), Some("line 5"));
}

#[test]
fn the_grid_marks_the_selected_cells() {
    let mut screen = Screen::new(12, 4);
    screen.feed(b"one two");
    screen.apply(Edit::Select {
        mode: SelectMode::Word,
        column: 4,
        row: 0,
        right: false,
    });
    assert_eq!(screen.grid().selection, [(0, 4..7)]);
}

#[test]
fn a_program_that_asks_for_mouse_reports_gets_the_report_kind_and_wire_form() {
    let mut screen = Screen::new(12, 4);
    assert_eq!(screen.grid().mouse, None);
    screen.feed(b"\x1b[?1000h\x1b[?1006h");
    assert_eq!(
        screen.grid().mouse,
        Some(Mouse {
            motion: Motion::Click,
            sgr: true,
        })
    );
    screen.feed(b"\x1b[?1006l\x1b[?1002h");
    assert_eq!(
        screen.grid().mouse,
        Some(Mouse {
            motion: Motion::Drag,
            sgr: false,
        })
    );
    screen.feed(b"\x1b[?1002l\x1b[?1003h");
    assert_eq!(
        screen.grid().mouse.map(|mouse| mouse.motion),
        Some(Motion::All)
    );
}

#[test]
fn bracketed_paste_is_reported_by_the_grid() {
    let mut screen = Screen::new(12, 4);
    assert!(!screen.bracketed_paste());
    screen.feed(b"\x1b[?2004h");
    assert!(screen.bracketed_paste());
}

#[test]
fn a_snapshot_keeps_unicode_wide_text_wrapping_and_history() {
    let mut screen = Screen::new(4, 3);
    screen.feed("one\r\n你\r\ne\u{0301}\r\n\x1b[31mabcd\x1b[0mW\r\n".as_bytes());
    let original = screen.transcript();
    assert!(original.contains('你'));
    assert!(original.contains("abcd"));
    let snap = screen.snapshot();
    assert!(snap.screen_kept);
    assert_eq!(snap.history_dropped, 0);
    assert!(snap.replay.len() <= SNAPSHOT_MAX);
    let restored = crate::Finished::restore(snap.cols, snap.rows, &snap.replay).expect("restore");
    assert_eq!(
        restored.transcript(),
        original,
        "replay {:?}",
        String::from_utf8_lossy(&snap.replay)
    );
    assert!(restored.grid().cursor.is_some());
    restored.select(crate::SelectMode::Word, 0, 0, false);
    assert!(restored.selection().is_some());
}

#[test]
fn a_snapshot_stays_within_the_total_bound_and_names_the_dropped_history() {
    let mut screen = Screen::new(20, 2);
    for index in 0..40 {
        screen.feed(format!("line-{index:02}-xxxxxxxx\r\n").as_bytes());
    }
    let visible = screen.draw().len();
    let limit = visible + 80;
    let snap = screen.snapshot_within(limit);
    assert!(snap.screen_kept);
    assert!(snap.replay.len() <= limit, "{}", snap.replay.len());
    assert!(snap.history_dropped > 0);
    let mut copy = Screen::new(snap.cols, snap.rows);
    copy.feed(&snap.replay);
    let text = copy.transcript();
    assert!(text.contains("line-39-xxxxxxxx"), "{text}");
    assert!(!text.contains("line-00-xxxxxxxx"), "{text}");
}

#[test]
fn a_visible_screen_over_the_bound_is_not_stored_as_a_whole_screen() {
    let mut screen = Screen::new(8, 2);
    screen.feed(b"hello\r\n");
    let snap = screen.snapshot_within(10);
    assert!(!snap.screen_kept);
    assert!(snap.replay.is_empty());
    assert_eq!(snap.history_dropped, 0);
}

#[test]
fn restore_refuses_geometry_and_payload_outside_the_bound() {
    assert!(crate::Finished::restore(1, 24, b"").is_none());
    assert!(crate::Finished::restore(MAX_COLS + 1, 24, b"").is_none());
    assert!(crate::Finished::restore(80, 0, b"").is_none());
    assert!(crate::Finished::restore(80, MAX_ROWS + 1, b"").is_none());
    let over = vec![0u8; SNAPSHOT_MAX + 1];
    assert!(crate::Finished::restore(80, 24, &over).is_none());
}
