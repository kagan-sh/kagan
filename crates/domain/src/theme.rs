use std::io::Read as _;
use std::path::Path;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The largest theme file kagan reads.
pub const MAX_THEME_BYTES: u64 = 64 * 1024;
const MAX_NAME: usize = 64;

/// An opaque color, written `#RRGGBB`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Color(u32);

impl Color {
    /// The color `0xRRGGBB`; higher bits are dropped.
    #[must_use]
    pub const fn new(rgb: u32) -> Self {
        Self(rgb & 0xff_ffff)
    }

    #[must_use]
    pub const fn rgb(self) -> u32 {
        self.0
    }

    #[must_use]
    pub fn hex(self) -> String {
        format!("#{:06x}", self.0)
    }
}

impl Serialize for Color {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.hex())
    }
}

impl<'de> Deserialize<'de> for Color {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let hex = text
            .strip_prefix('#')
            .filter(|hex| hex.len() == 6 && hex.bytes().all(|byte| byte.is_ascii_hexdigit()));
        hex.and_then(|hex| u32::from_str_radix(hex, 16).ok())
            .map(Self)
            .ok_or_else(|| serde::de::Error::custom("expected #RRGGBB"))
    }
}

/// The colors of one mode of a theme.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Palette {
    pub bg: Color,
    pub surface: Color,
    pub raised: Color,
    pub ink: Color,
    pub muted: Color,
    pub line: Color,
    pub accent: Color,
    pub primary_foreground: Color,
    pub working: Color,
    pub needs: Color,
    pub failed: Color,
    pub idle: Color,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hover: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal: Option<TerminalColors>,
}

impl Palette {
    /// The terminal colors. A theme without them keeps the xterm colors that programs expect.
    #[must_use]
    pub fn terminal(&self) -> TerminalColors {
        self.terminal.unwrap_or_else(|| xterm(self.bg, self.ink))
    }

    /// Added lines and counts. A theme without `added` uses `working`.
    #[must_use]
    pub fn added(&self) -> Color {
        self.added.unwrap_or(self.working)
    }
}

/// A terminal's default colors and its 16 named colors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TerminalColors {
    pub bg: Color,
    pub fg: Color,
    pub cursor: Color,
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,
    pub bright_black: Color,
    pub bright_red: Color,
    pub bright_green: Color,
    pub bright_yellow: Color,
    pub bright_blue: Color,
    pub bright_magenta: Color,
    pub bright_cyan: Color,
    pub bright_white: Color,
}

fn xterm(bg: Color, fg: Color) -> TerminalColors {
    TerminalColors {
        bg,
        fg,
        cursor: fg,
        black: Color(0x00_0000),
        red: Color(0xcd_0000),
        green: Color(0x00_cd00),
        yellow: Color(0xcd_cd00),
        blue: Color(0x00_00ee),
        magenta: Color(0xcd_00cd),
        cyan: Color(0x00_cdcd),
        white: Color(0xe5_e5e5),
        bright_black: Color(0x7f_7f7f),
        bright_red: Color(0xff_0000),
        bright_green: Color(0x00_ff00),
        bright_yellow: Color(0xff_ff00),
        bright_blue: Color(0x5c_5cff),
        bright_magenta: Color(0xff_00ff),
        bright_cyan: Color(0x00_ffff),
        bright_white: Color(0xff_ffff),
    }
}

/// A theme file: `<dataDir>/themes/<id>.json`, or a file a plugin declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    pub name: String,
    pub light: Palette,
    pub dark: Palette,
}

#[derive(Debug, thiserror::Error)]
pub enum ThemeFileError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("not a regular file")]
    NotAFile,
    #[error("larger than {} KiB", MAX_THEME_BYTES / 1024)]
    TooBig,
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error("theme name is empty")]
    BlankName,
    #[error("theme name must be one line of at most {MAX_NAME} characters")]
    BadName,
}

impl ThemeFile {
    /// Reads a theme file: a regular file of at most [`MAX_THEME_BYTES`].
    ///
    /// # Errors
    /// When the path is not such a file, or it is not a theme file.
    pub fn read(path: &Path) -> Result<Self, ThemeFileError> {
        let meta = std::fs::metadata(path)?;
        if !meta.is_file() {
            return Err(ThemeFileError::NotAFile);
        }
        if meta.len() > MAX_THEME_BYTES {
            return Err(ThemeFileError::TooBig);
        }
        let mut bytes = Vec::new();
        std::fs::File::open(path)?
            .take(MAX_THEME_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_THEME_BYTES {
            return Err(ThemeFileError::TooBig);
        }
        Self::parse(&bytes)
    }

    /// Reads theme file bytes.
    ///
    /// # Errors
    /// When the bytes are not a theme file, or its name is blank, longer than 64 characters or
    /// more than one line.
    pub fn parse(bytes: &[u8]) -> Result<Self, ThemeFileError> {
        let file: Self = serde_json::from_slice(bytes)?;
        if file.name.trim().is_empty() {
            return Err(ThemeFileError::BlankName);
        }
        if file.name.chars().count() > MAX_NAME || file.name.chars().any(char::is_control) {
            return Err(ThemeFileError::BadName);
        }
        Ok(file)
    }
}
