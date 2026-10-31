use alacritty_terminal::term::cell::Cell as TermCell;

use super::*;
use crate::screen::Screen;

fn grid(text: &str) -> Grid {
    let mut screen = Screen::new(6, 3);
    screen.feed(text.as_bytes());
    screen.grid()
}

fn first(text: &str) -> Cell {
    grid(text).lines[0][0].clone()
}

fn text(grid: &Grid, line: usize) -> String {
    grid.lines[line].iter().map(|cell| cell.c).collect()
}

#[test]
fn the_grid_has_every_line_and_column() {
    let grid = grid("one\r\ntwo");
    assert_eq!(grid.lines.len(), 3);
    assert!(grid.lines.iter().all(|line| line.len() == 6));
    assert_eq!(text(&grid, 0), "one   ");
    assert_eq!(text(&grid, 1), "two   ");
}

#[test]
fn the_cursor_shows_where_the_program_left_it() {
    let shown = grid("\x1b[2;4H").cursor.unwrap();
    assert_eq!((shown.line, shown.column), (1, 3));
    assert_eq!(grid("\x1b[?25l").cursor, None);
}

#[test]
fn default_colors_are_left_to_the_client() {
    let cell = first("x");
    assert_eq!((cell.fg, cell.bg), (Color::Foreground, Color::Background));
}

#[test]
fn numbered_colors_resolve_to_slots_or_rgb() {
    for (code, want) in [
        ("31", Color::Ansi(Ansi::Red)),
        ("94", Color::Ansi(Ansi::BrightBlue)),
        ("38;5;16", Color::Rgb(0, 0, 0)),
        ("38;5;67", Color::Rgb(95, 135, 175)),
        ("38;5;231", Color::Rgb(255, 255, 255)),
        ("38;5;232", Color::Rgb(8, 8, 8)),
        ("38;5;255", Color::Rgb(238, 238, 238)),
        ("38;2;1;2;3", Color::Rgb(1, 2, 3)),
    ] {
        assert_eq!(first(&format!("\x1b[{code}mx")).fg, want, "{code}");
    }
    assert_eq!(first("\x1b[42mx").bg, Color::Ansi(Ansi::Green));
}

#[test]
fn a_color_the_program_set_wins_over_the_default() {
    assert_eq!(
        first("\x1b]4;1;rgb:12/34/56\x07\x1b[31mx").fg,
        Color::Rgb(0x12, 0x34, 0x56)
    );
}

#[test]
fn dim_and_bright_names_resolve() {
    let colors = Colors::default();
    let named = |name| color(TermColor::Named(name), &colors);
    assert_eq!(named(NamedColor::DimRed), Color::Ansi(Ansi::Red));
    assert_eq!(named(NamedColor::DimWhite), Color::Ansi(Ansi::White));
    assert_eq!(named(NamedColor::BrightForeground), Color::Foreground);
    assert_eq!(named(NamedColor::DimForeground), Color::Foreground);
    assert_eq!(named(NamedColor::Cursor), Color::Foreground);
    assert_eq!(named(NamedColor::Background), Color::Background);
}

#[test]
fn inverse_swaps_the_colors() {
    let cell = first("\x1b[7;31mx");
    assert_eq!(
        (cell.fg, cell.bg),
        (Color::Background, Color::Ansi(Ansi::Red))
    );
}

#[test]
fn attributes_reach_the_cell() {
    assert_eq!(first("\x1b[1mx").weight, Weight::Bold);
    assert_eq!(first("\x1b[2mx").weight, Weight::Dim);
    assert_eq!(first("x").weight, Weight::Normal);
    assert!(first("\x1b[3mx").italic);
    assert!(first("\x1b[4:3mx").underline);
    assert!(first("\x1b[9mx").strikeout);
    let plain = first("x");
    assert!(!plain.italic && !plain.underline && !plain.strikeout);
    assert_eq!(first("\x1b[8mx").c, ' ');
}

#[test]
fn a_wide_character_leaves_a_space_after_it() {
    let grid = grid("日x");
    assert_eq!(&text(&grid, 0)[..], "日 x   ");
}

#[test]
fn combining_marks_stay_with_their_character() {
    let cell = first("e\u{301}");
    assert_eq!((cell.c, cell.zerowidth), ('e', vec!['\u{301}']));
    assert!(first("e").zerowidth.is_empty());
}

#[test]
fn the_modes_that_change_keys_are_reported() {
    let plain = grid("");
    assert!(!plain.app_cursor && !plain.bracketed_paste);
    assert!(grid("\x1b[?1h").app_cursor);
    assert!(grid("\x1b[?2004h").bracketed_paste);
}

#[test]
fn a_blank_cell_is_a_space() {
    let cell = cell(&TermCell::default(), &Colors::default());
    assert_eq!(cell.c, ' ');
}
