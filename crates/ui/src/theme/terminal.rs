use kg_core::Ansi;
use kg_domain::{Color, TerminalColors};

pub(crate) fn ansi(colors: &TerminalColors, name: Ansi) -> Color {
    match name {
        Ansi::Black => colors.black,
        Ansi::Red => colors.red,
        Ansi::Green => colors.green,
        Ansi::Yellow => colors.yellow,
        Ansi::Blue => colors.blue,
        Ansi::Magenta => colors.magenta,
        Ansi::Cyan => colors.cyan,
        Ansi::White => colors.white,
        Ansi::BrightBlack => colors.bright_black,
        Ansi::BrightRed => colors.bright_red,
        Ansi::BrightGreen => colors.bright_green,
        Ansi::BrightYellow => colors.bright_yellow,
        Ansi::BrightBlue => colors.bright_blue,
        Ansi::BrightMagenta => colors.bright_magenta,
        Ansi::BrightCyan => colors.bright_cyan,
        Ansi::BrightWhite => colors.bright_white,
    }
}
