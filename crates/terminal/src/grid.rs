use std::ops::Range;

use alacritty_terminal::event::EventListener;
use alacritty_terminal::grid::Dimensions;
use alacritty_terminal::term::cell::{Cell as TermCell, Flags};
use alacritty_terminal::term::color::Colors;
use alacritty_terminal::term::{Term, TermMode};
use alacritty_terminal::vte::ansi::{Color as TermColor, NamedColor, Rgb};

/// The view of the screen as cells, for a client in the same process. It holds only the rows on
/// screen, at the place the person scrolled to.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Grid {
    pub lines: Vec<Vec<Cell>>,
    /// Hidden when the program hides it, or when it is scrolled off the view.
    pub cursor: Option<Cursor>,
    /// Arrow keys send `ESC O` instead of `ESC [`.
    pub app_cursor: bool,
    pub bracketed_paste: bool,
    /// Set when the program asked for mouse reports.
    pub mouse: Option<Mouse>,
    /// The selected columns of each line: (line, columns).
    pub selection: Vec<(usize, Range<usize>)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cursor {
    pub line: usize,
    pub column: usize,
}

/// The mouse reports a program asked for. `sgr` is the wire form (1006); without it, the legacy one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouse {
    pub motion: Motion,
    pub sgr: bool,
}

/// Which mouse movements a program wants: none but clicks, moves while a button is held, or all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    Click,
    Drag,
    All,
}

/// One column. The column after a wide character holds a space.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cell {
    pub c: char,
    pub zerowidth: Vec<char>,
    pub fg: Color,
    pub bg: Color,
    pub weight: Weight,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}

/// Inverse video is already applied.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    /// The client's own text color.
    Foreground,
    /// The client's own background.
    Background,
    /// One of the 16 ANSI colors. The client colors it from its theme.
    Ansi(Ansi),
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ansi {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Ansi {
    const ALL: [Self; 16] = [
        Self::Black,
        Self::Red,
        Self::Green,
        Self::Yellow,
        Self::Blue,
        Self::Magenta,
        Self::Cyan,
        Self::White,
        Self::BrightBlack,
        Self::BrightRed,
        Self::BrightGreen,
        Self::BrightYellow,
        Self::BrightBlue,
        Self::BrightMagenta,
        Self::BrightCyan,
        Self::BrightWhite,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Weight {
    Normal,
    Bold,
    Dim,
}

pub(crate) fn grid<T: EventListener>(term: &Term<T>) -> Grid {
    let content = term.renderable_content();
    let offset = i32::try_from(content.display_offset).unwrap_or_default();
    let rows = term.screen_lines();
    let mut lines = vec![Vec::with_capacity(term.columns()); rows];
    let mut selection: Vec<(usize, Range<usize>)> = Vec::new();
    for indexed in content.display_iter {
        let row = usize::try_from(indexed.point.line.0 + offset).ok();
        let Some(line) = row.and_then(|row| lines.get_mut(row).map(|line| (row, line))) else {
            continue;
        };
        if content
            .selection
            .as_ref()
            .is_some_and(|range| range.contains(indexed.point))
        {
            let column = indexed.point.column.0;
            match selection.last_mut() {
                Some((row, columns)) if *row == line.0 && columns.end == column => {
                    columns.end += 1;
                }
                _ => selection.push((line.0, column..column + 1)),
            }
        }
        line.1.push(cell(indexed.cell, content.colors));
    }
    let point = content.cursor.point;
    let mode = content.mode;
    let cursor_row = usize::try_from(point.line.0 + offset).ok();
    Grid {
        lines,
        cursor: mode
            .contains(TermMode::SHOW_CURSOR)
            .then_some(cursor_row)
            .flatten()
            .filter(|line| *line < rows)
            .map(|line| Cursor {
                line,
                column: point.column.0,
            }),
        app_cursor: mode.contains(TermMode::APP_CURSOR),
        bracketed_paste: mode.contains(TermMode::BRACKETED_PASTE),
        selection,
        mouse: mode.intersects(TermMode::MOUSE_MODE).then(|| Mouse {
            motion: if mode.contains(TermMode::MOUSE_MOTION) {
                Motion::All
            } else if mode.contains(TermMode::MOUSE_DRAG) {
                Motion::Drag
            } else {
                Motion::Click
            },
            sgr: mode.contains(TermMode::SGR_MOUSE),
        }),
    }
}

fn cell(cell: &TermCell, colors: &Colors) -> Cell {
    let flags = cell.flags;
    let (mut fg, mut bg) = (color(cell.fg, colors), color(cell.bg, colors));
    if flags.contains(Flags::INVERSE) {
        (fg, bg) = (bg, fg);
    }
    Cell {
        c: if flags.contains(Flags::HIDDEN) {
            ' '
        } else {
            cell.c
        },
        zerowidth: cell.zerowidth().map(<[char]>::to_vec).unwrap_or_default(),
        fg,
        bg,
        weight: if flags.contains(Flags::BOLD) {
            Weight::Bold
        } else if flags.contains(Flags::DIM) {
            Weight::Dim
        } else {
            Weight::Normal
        },
        italic: flags.contains(Flags::ITALIC),
        underline: flags.intersects(Flags::ALL_UNDERLINES),
        strikeout: flags.contains(Flags::STRIKEOUT),
    }
}

fn color(color: TermColor, colors: &Colors) -> Color {
    let index = match color {
        TermColor::Spec(rgb) => return rgb_color(rgb),
        TermColor::Indexed(index) => usize::from(index),
        TermColor::Named(
            NamedColor::Foreground
            | NamedColor::BrightForeground
            | NamedColor::DimForeground
            | NamedColor::Cursor,
        ) => return Color::Foreground,
        TermColor::Named(NamedColor::Background) => return Color::Background,
        TermColor::Named(name) => match name as usize {
            dim @ 259..267 => dim - 259,
            index => index,
        },
    };
    colors[index].map_or_else(|| palette(index), rgb_color)
}

fn rgb_color(Rgb { r, g, b }: Rgb) -> Color {
    Color::Rgb(r, g, b)
}

const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn palette(index: usize) -> Color {
    if let Some(name) = Ansi::ALL.get(index) {
        return Color::Ansi(*name);
    }
    let (r, g, b) = if index < 232 {
        let cube = index - 16;
        (CUBE[cube / 36], CUBE[cube / 6 % 6], CUBE[cube % 6])
    } else {
        let level = u8::try_from(8 + 10 * (index.min(255) - 232)).unwrap_or(u8::MAX);
        (level, level, level)
    };
    Color::Rgb(r, g, b)
}

#[cfg(test)]
mod tests;
