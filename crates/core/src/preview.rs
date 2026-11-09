use std::io::Read as _;
use std::path::Path;

use gpui_kit::{AppContext as _, AsyncApp};
use kg_domain::{
    OpError, OpResult, PreviewCut, PreviewFile, PreviewImageFormat, PreviewKind, PreviewOp,
};

use crate::host::database::{Database, Result};

mod csv;

const TEXT_LIMIT: u64 = 1024 * 1024;
const IMAGE_LIMIT: u64 = 8 * 1024 * 1024;
const PIXEL_LIMIT: u64 = 40_000_000;
const LINE_LIMIT: usize = 2_000;
const LINE_BYTES: usize = 4_000;
const BINARY_SNIFF: usize = 8 * 1024;
const PNG: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
const JPEG: [u8; 3] = [0xff, 0xd8, 0xff];

pub(crate) async fn run(database: Database, op: PreviewOp, cx: &mut AsyncApp) -> Result<OpResult> {
    let PreviewOp::Show { thread_id, path } = op;
    let root = match &thread_id {
        Some(id) => Some(crate::thread::ready_environment(&database, id).await?.path),
        None => None,
    };
    cx.background_spawn(async move { read(root.as_deref(), &path).map(OpResult::Preview) })
        .await
}

fn read(root: Option<&Path>, named: &str) -> Result<PreviewFile> {
    let root = root
        .map(dunce::canonicalize)
        .transpose()
        .map_err(|_| OpError::Failed("the thread's folder is gone".into()))?;
    let requested = Path::new(named);
    let absolute = match &root {
        Some(root) => {
            let joined = if requested.is_absolute() {
                requested.to_path_buf()
            } else {
                root.join(requested)
            };
            let absolute = dunce::canonicalize(&joined).map_err(|_| not_found(named))?;
            if !absolute.starts_with(root) {
                return Err(OpError::Invalid(format!(
                    "{named} is outside this thread's folder"
                )));
            }
            absolute
        }
        None if requested.is_absolute() => {
            dunce::canonicalize(requested).map_err(|_| not_found(named))?
        }
        None => {
            return Err(OpError::Invalid(
                "a relative path needs a thread; name an absolute path or set KG_THREAD_ID".into(),
            ));
        }
    };
    let metadata = std::fs::metadata(&absolute)?;
    if metadata.is_dir() {
        return Err(OpError::Invalid(format!("{named} is a folder")));
    }
    if !metadata.is_file() {
        return Err(OpError::Invalid(format!("{named} is not a regular file")));
    }
    let size = metadata.len();
    let shown = shown(root.as_deref(), &absolute, named);
    let kind = kind(&absolute, size)?;
    Ok(PreviewFile {
        path: shown,
        absolute,
        bytes: size,
        kind,
    })
}

fn shown(root: Option<&Path>, absolute: &Path, named: &str) -> String {
    let shown = root
        .and_then(|root| absolute.strip_prefix(root).ok())
        .map_or_else(
            || absolute.display().to_string(),
            |relative| relative.display().to_string(),
        );
    if shown.is_empty() {
        named.to_owned()
    } else {
        shown
    }
}

fn kind(absolute: &Path, size: u64) -> Result<PreviewKind> {
    let prefix = head(absolute, 16)?;
    let image = if prefix.starts_with(&PNG) {
        Some(PreviewImageFormat::Png)
    } else if prefix.starts_with(&JPEG) {
        Some(PreviewImageFormat::Jpeg)
    } else {
        None
    };
    if let Some(format) = image {
        if size > IMAGE_LIMIT {
            return Ok(too_big(size, "images"));
        }
        let bytes = head(absolute, IMAGE_LIMIT + 1)?;
        let Some((width, height)) = image_size(&bytes) else {
            return Ok(unavailable("This image could not be read."));
        };
        if u64::from(width) * u64::from(height) > PIXEL_LIMIT {
            return Ok(unavailable(format!(
                "{width}×{height} is too many pixels to preview."
            )));
        }
        return Ok(PreviewKind::Image {
            format,
            width,
            height,
        });
    }
    let extension = absolute
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default()
        .to_ascii_lowercase();
    if extension == "pdf" {
        return Ok(unavailable("PDF is not previewed yet."));
    }
    let noun = if matches!(extension.as_str(), "csv" | "tsv") {
        "CSV"
    } else {
        "text"
    };
    if size > TEXT_LIMIT {
        return Ok(too_big(size, noun));
    }
    let bytes = head(absolute, TEXT_LIMIT + 1)?;
    if bytes.len() as u64 > TEXT_LIMIT {
        return Ok(too_big(size, noun));
    }
    if bytes[..bytes.len().min(BINARY_SNIFF)].contains(&0) {
        return Ok(unavailable(
            "Not text. Preview shows text, Markdown, images and CSV.",
        ));
    }
    let Ok(text) = String::from_utf8(bytes) else {
        return Ok(unavailable("Not UTF-8. kagan does not guess an encoding."));
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let text = text.replace("\r\n", "\n");
    if text.is_empty() {
        return Ok(unavailable("Empty file."));
    }
    Ok(match extension.as_str() {
        "md" | "markdown" => text_kind(&text, false),
        "csv" => csv_kind(&text, ','),
        "tsv" => csv_kind(&text, '\t'),
        _ => text_kind(&text, true),
    })
}

fn text_kind(text: &str, plain: bool) -> PreviewKind {
    let total = text.lines().count();
    let mut shown = String::new();
    for (index, line) in text.lines().take(LINE_LIMIT).enumerate() {
        if index > 0 {
            shown.push('\n');
        }
        shown.push_str(cut_line(line));
    }
    let cut = (total > LINE_LIMIT).then_some(PreviewCut {
        shown: LINE_LIMIT,
        total,
    });
    if plain {
        PreviewKind::Text { text: shown, cut }
    } else {
        PreviewKind::Markdown { text: shown, cut }
    }
}

fn csv_kind(text: &str, delimiter: char) -> PreviewKind {
    let table = csv::parse(text, delimiter);
    PreviewKind::Csv {
        rows: table.rows,
        rows_total: table.rows_total,
        columns_total: table.columns_total,
        truncated: table.truncated,
    }
}

fn cut_line(line: &str) -> &str {
    if line.len() <= LINE_BYTES {
        return line;
    }
    let mut end = LINE_BYTES;
    while !line.is_char_boundary(end) {
        end -= 1;
    }
    &line[..end]
}

fn too_big(size: u64, noun: &str) -> PreviewKind {
    unavailable(format!(
        "{}. Preview shows {noun} up to {}.",
        human_size(size),
        if noun == "images" { "8 MB" } else { "1 MB" }
    ))
}

fn unavailable(reason: impl Into<String>) -> PreviewKind {
    PreviewKind::Unavailable {
        reason: reason.into(),
    }
}

fn not_found(named: &str) -> OpError {
    OpError::NotFound(format!("no file at {named}"))
}

fn head(path: &Path, limit: u64) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes)?;
    Ok(bytes)
}

/// The bytes of an image the window draws, under the caps `PreviewOp::Show` checks. A file
/// outside `root` is refused when there is one.
#[must_use]
pub fn preview_image(root: Option<&Path>, absolute: &Path) -> Option<Vec<u8>> {
    let absolute = dunce::canonicalize(absolute).ok()?;
    if let Some(root) = root {
        let root = dunce::canonicalize(root).ok()?;
        if !absolute.starts_with(root) {
            return None;
        }
    }
    if !std::fs::metadata(&absolute).ok()?.is_file() {
        return None;
    }
    let bytes = head(&absolute, IMAGE_LIMIT + 1).ok()?;
    if bytes.len() as u64 > IMAGE_LIMIT {
        return None;
    }
    let (width, height) = image_size(&bytes)?;
    (u64::from(width) * u64::from(height) <= PIXEL_LIMIT).then_some(bytes)
}

/// One sentence for each dimension that was cut, or `None` when nothing was.
#[must_use]
pub fn preview_note(kind: &PreviewKind) -> Option<String> {
    match kind {
        PreviewKind::Text { cut, .. } | PreviewKind::Markdown { cut, .. } => {
            cut.as_ref().map(|cut| {
                format!(
                    "Showing the first {} of {} lines.",
                    count(cut.shown),
                    count(cut.total)
                )
            })
        }
        PreviewKind::Csv {
            rows,
            rows_total,
            columns_total,
            ..
        } => {
            let rows_shown = rows.len();
            let columns_shown = rows.iter().map(Vec::len).max().unwrap_or(0);
            let mut parts = Vec::new();
            if rows_shown < *rows_total {
                parts.push(format!(
                    "Showing the first {} of {} rows.",
                    count(rows_shown),
                    count(*rows_total)
                ));
            }
            if columns_shown < *columns_total {
                parts.push(format!(
                    "Showing the first {} of {} columns.",
                    count(columns_shown),
                    count(*columns_total)
                ));
            }
            (!parts.is_empty()).then(|| parts.join(" "))
        }
        PreviewKind::Image { .. } | PreviewKind::Unavailable { .. } => None,
    }
}

fn count(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    out
}

/// Bytes as a person reads them: `8 B`, `1.4 KB` or `2.3 MB`.
#[must_use]
pub fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    if bytes < KB {
        format!("{bytes} B")
    } else if bytes < MB {
        format!("{}.{} KB", bytes / KB, bytes % KB * 10 / KB)
    } else {
        format!("{}.{} MB", bytes / MB, bytes % MB * 10 / MB)
    }
}

/// The pixel size a PNG or JPEG header names, for the caps check.
#[must_use]
pub fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}
