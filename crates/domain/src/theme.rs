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
    pub selection: Option<Color>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input: Option<Color>,
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

    /// Selected text. A theme without `selection` uses `raised`.
    #[must_use]
    pub fn selection(&self) -> Color {
        self.selection.unwrap_or(self.raised)
    }

    /// Fields and other inputs. A theme without `input` uses `line`.
    #[must_use]
    pub fn input(&self) -> Color {
        self.input.unwrap_or(self.line)
    }
}

/// The six spacing steps of the window, from smallest to largest, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Space {
    pub xs: u16,
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
    pub xl: u16,
    pub xxl: u16,
}

impl Default for Space {
    fn default() -> Self {
        Self {
            xs: 4,
            sm: 8,
            md: 12,
            lg: 16,
            xl: 24,
            xxl: 32,
        }
    }
}

/// The four corner radii of the window, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Radius {
    pub xs: u16,
    pub sm: u16,
    pub md: u16,
    pub lg: u16,
}

impl Default for Radius {
    fn default() -> Self {
        Self {
            xs: 6,
            sm: 8,
            md: 8,
            lg: 12,
        }
    }
}

/// The five text sizes of the window, in pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct TypeSizes {
    pub caption: u16,
    pub small: u16,
    pub body: u16,
    pub title: u16,
    pub mono: u16,
}

impl Default for TypeSizes {
    fn default() -> Self {
        Self {
            caption: 11,
            small: 12,
            body: 13,
            title: 15,
            mono: 12,
        }
    }
}

/// The density and type sizes a theme gives the host and its web panels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields, default)]
pub struct Look {
    pub space: Space,
    pub radius: Radius,
    #[serde(rename = "type")]
    pub type_sizes: TypeSizes,
    /// The height of a list row, such as a thread row, in pixels.
    pub row: u16,
    /// The height of a compact control, such as a sidebar project row, in pixels.
    pub control: u16,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            space: Space::default(),
            radius: Radius::default(),
            type_sizes: TypeSizes::default(),
            row: 28,
            control: 24,
        }
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

/// The syntax colors a theme ships for the review and the Editor, in Zed's theme format.
///
/// kagan keeps them as JSON: GPUI Kit's `HighlightTheme` reads them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Highlight {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dark: Option<serde_json::Value>,
}

/// A theme file: `<dataDir>/themes/<id>.json`, or a file a plugin declares.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFile {
    pub name: String,
    pub light: Palette,
    pub dark: Palette,
    /// Density and type sizes, the same in both modes.
    #[serde(default)]
    pub look: Look,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<Highlight>,
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
    #[error("theme {field} must be {min} to {max}")]
    Look { field: String, min: u16, max: u16 },
    #[error("theme name must be one line of at most {MAX_NAME} characters")]
    BadName,
    #[error("{0}")]
    Highlight(String),
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
    /// When the bytes are not a theme file, its name is blank, longer than 64 characters or
    /// more than one line, or its `highlight` section is not a Zed theme.
    pub fn parse(bytes: &[u8]) -> Result<Self, ThemeFileError> {
        let file: Self = serde_json::from_slice(bytes)?;
        if file.name.trim().is_empty() {
            return Err(ThemeFileError::BlankName);
        }
        if file.name.chars().count() > MAX_NAME || file.name.chars().any(char::is_control) {
            return Err(ThemeFileError::BadName);
        }
        check_look(&file.look)?;
        if let Some(highlight) = &file.highlight {
            for (field, value) in [
                ("highlight.light", &highlight.light),
                ("highlight.dark", &highlight.dark),
            ] {
                if let Some(value) = value {
                    check_highlight(field, value)?;
                }
            }
        }
        Ok(file)
    }
}

fn check_look(look: &Look) -> Result<(), ThemeFileError> {
    for (field, value) in [
        ("space.xs", look.space.xs),
        ("space.sm", look.space.sm),
        ("space.md", look.space.md),
        ("space.lg", look.space.lg),
        ("space.xl", look.space.xl),
        ("space.xxl", look.space.xxl),
    ] {
        in_range(field, value, 0..=64)?;
    }
    for (field, value) in [
        ("radius.xs", look.radius.xs),
        ("radius.sm", look.radius.sm),
        ("radius.md", look.radius.md),
        ("radius.lg", look.radius.lg),
    ] {
        in_range(field, value, 0..=32)?;
    }
    for (field, value) in [
        ("type.caption", look.type_sizes.caption),
        ("type.small", look.type_sizes.small),
        ("type.body", look.type_sizes.body),
        ("type.title", look.type_sizes.title),
        ("type.mono", look.type_sizes.mono),
    ] {
        in_range(field, value, 8..=48)?;
    }
    for (field, value) in [("row", look.row), ("control", look.control)] {
        in_range(field, value, 16..=64)?;
    }
    Ok(())
}

fn in_range(
    field: &str,
    value: u16,
    range: std::ops::RangeInclusive<u16>,
) -> Result<(), ThemeFileError> {
    if range.contains(&value) {
        return Ok(());
    }
    Err(ThemeFileError::Look {
        field: format!("look.{field}"),
        min: *range.start(),
        max: *range.end(),
    })
}

// Checks what GPUI Kit's Zed `HighlightTheme` deserialization requires, so a theme that
// passes also loads there, and names the field that is wrong.
fn check_highlight(field: &str, value: &serde_json::Value) -> Result<(), ThemeFileError> {
    const COLOR: &str = "#RGB, #RGBA, #RRGGBB or #RRGGBBAA";
    let bad = ThemeFileError::Highlight;
    let Some(theme) = value.as_object() else {
        return Err(bad(format!("{field} must be an object")));
    };
    if !theme.get("name").is_some_and(serde_json::Value::is_string) {
        return Err(bad(format!("{field}.name must be a string")));
    }
    if let Some(appearance) = theme.get("appearance")
        && !matches!(appearance.as_str(), Some("light" | "dark"))
    {
        return Err(bad(format!(
            "{field}.appearance must be \"light\" or \"dark\""
        )));
    }
    let Some(style) = theme.get("style").and_then(serde_json::Value::as_object) else {
        return Err(bad(format!("{field}.style must be an object")));
    };
    let Some(syntax) = style.get("syntax").and_then(serde_json::Value::as_object) else {
        return Err(bad(format!("{field}.style.syntax must be an object")));
    };
    for (name, token) in syntax {
        let field = format!("{field}.style.syntax.{name}");
        let Some(token) = token.as_object() else {
            return Err(bad(format!("{field} must be an object")));
        };
        if let Some(color) = token.get("color")
            && !color.is_null()
            && !is_hex(color)
        {
            return Err(bad(format!("{field}.color must be a {COLOR} color")));
        }
        if let Some(font) = token.get("font_style")
            && !font.is_null()
            && !matches!(font.as_str(), Some("normal" | "italic" | "underline"))
        {
            return Err(bad(format!(
                "{field}.font_style must be \"normal\", \"italic\" or \"underline\""
            )));
        }
        if let Some(weight) = token.get("font_weight")
            && !weight.is_null()
            && !matches!(
                weight.as_u64(),
                Some(100 | 200 | 300 | 400 | 500 | 600 | 700 | 800 | 900)
            )
        {
            return Err(bad(format!(
                "{field}.font_weight must be 100 to 900 in steps of 100"
            )));
        }
    }
    for (name, color) in style {
        if name != "syntax" && !color.is_null() && !is_hex(color) {
            return Err(bad(format!("{field}.style.{name} must be a {COLOR} color")));
        }
    }
    Ok(())
}

fn is_hex(value: &serde_json::Value) -> bool {
    let Some(hex) = value
        .as_str()
        .map(str::trim)
        .and_then(|text| text.strip_prefix('#'))
    else {
        return false;
    };
    matches!(hex.len(), 3 | 4 | 6 | 8) && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
}
