use gpui_kit::component::ThemeMode;
use kg_domain::{Color, Palette, TerminalColors, ThemeFile};

use super::catalog::{CatalogEntry, Source};

pub(super) fn palette(file: &ThemeFile, mode: ThemeMode) -> &Palette {
    match mode {
        ThemeMode::Light => &file.light,
        ThemeMode::Dark => &file.dark,
    }
}

struct Swatch {
    bg: u32,
    surface: u32,
    raised: u32,
    ink: u32,
    muted: u32,
    line: u32,
    accent: u32,
    needs: u32,
    failed: u32,
    hover: Option<u32>,
    added: Option<u32>,
    working: u32,
    terminal: Option<TerminalColors>,
}

impl Swatch {
    fn palette(self) -> Palette {
        Palette {
            bg: Color::new(self.bg),
            surface: Color::new(self.surface),
            raised: Color::new(self.raised),
            ink: Color::new(self.ink),
            muted: Color::new(self.muted),
            line: Color::new(self.line),
            accent: Color::new(self.accent),
            primary_foreground: Color::new(self.bg),
            working: Color::new(self.working),
            needs: Color::new(self.needs),
            failed: Color::new(self.failed),
            idle: Color::new(self.muted),
            hover: self.hover.map(Color::new),
            added: self.added.map(Color::new),
            terminal: self.terminal,
        }
    }
}

pub(super) const FJORD: TerminalColors = TerminalColors {
    bg: Color::new(0x1a_1d24),
    fg: Color::new(0xe5_e9f0),
    cursor: Color::new(0xe5_e9f0),
    black: Color::new(0x3b_4252),
    red: Color::new(0xec_959c),
    green: Color::new(0xa8_c98e),
    yellow: Color::new(0xeb_c874),
    blue: Color::new(0x7f_a6e8),
    magenta: Color::new(0xd0_9ad6),
    cyan: Color::new(0x8f_c7d6),
    white: Color::new(0xd8_dee9),
    bright_black: Color::new(0x98_a0b0),
    bright_red: Color::new(0xf4_aab0),
    bright_green: Color::new(0xbc_d8a6),
    bright_yellow: Color::new(0xf2_d898),
    bright_blue: Color::new(0xa3_bff0),
    bright_magenta: Color::new(0xe0_b6e4),
    bright_cyan: Color::new(0xa8_d6e2),
    bright_white: Color::new(0xec_eff4),
};

pub(super) fn fjord() -> ThemeFile {
    ThemeFile {
        name: "Ember Fjord".into(),
        light: Swatch {
            bg: 0xee_f1f5,
            surface: 0xe4_e8ee,
            raised: 0xd8_dee7,
            ink: 0x26_2b36,
            muted: 0x53_5c6c,
            line: 0xd3_d9e2,
            accent: 0x9a_4116,
            needs: 0x80_570a,
            failed: 0xa6_2f3c,
            hover: Some(0xdd_e3eb),
            added: Some(0x3c_6a2c),
            working: 0x28_5f73,
            terminal: Some(FJORD),
        }
        .palette(),
        dark: Swatch {
            bg: 0x22_262f,
            surface: 0x1c_1f27,
            raised: 0x2f_3440,
            ink: 0xe5_e9f0,
            muted: 0xa7_afbf,
            line: 0x35_3b48,
            accent: 0xf0_905f,
            needs: 0xeb_c874,
            failed: 0xec_959c,
            hover: Some(0x27_2c36),
            added: Some(0xa8_c98e),
            working: 0x8f_c7d6,
            terminal: Some(FJORD),
        }
        .palette(),
    }
}

pub(super) fn builtins() -> Vec<CatalogEntry> {
    vec![
        CatalogEntry {
            id: "fjord".into(),
            source: Source::Builtin,
            file: Ok(fjord()),
        },
        builtin(
            "ember",
            include_bytes!("../../../../docs/themes/ember.json"),
        ),
        builtin("grey", include_bytes!("../../../../docs/themes/grey.json")),
    ]
}

pub(super) fn builtin(id: &str, bytes: &[u8]) -> CatalogEntry {
    CatalogEntry {
        id: id.into(),
        source: Source::Builtin,
        file: ThemeFile::parse(bytes).map_err(|error| error.to_string()),
    }
}

pub(super) fn reserved(id: &str) -> bool {
    matches!(id, "fjord" | "ember" | "grey")
}
