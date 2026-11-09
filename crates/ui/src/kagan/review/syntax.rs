use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::ops::Range;
use std::time::Duration;

use gpui_kit::component::highlighter::{HighlightTheme, SyntaxHighlighter};
use gpui_kit::component::input::{HighlightStyleResolver, Rope, RopeExt as _};
use gpui_kit::{HighlightStyle, Hsla, StyledText};
use kg_domain::{DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffSide};
use tree_sitter::{InputEdit, Point};

const MAX_LINE: usize = 1000;
const PARSE_BUDGET: Duration = Duration::from_millis(200);

type Captures = Vec<(Range<usize>, String)>;

#[derive(Default)]
pub(super) struct Syntax {
    lines: BTreeMap<(u8, u32), Captures>,
}

impl Syntax {
    pub(super) fn clear(&mut self) {
        self.lines.clear();
    }

    pub(super) fn spans(&self, side: DiffSide, line: u32) -> &[(Range<usize>, String)] {
        self.lines.get(&key(side, line)).map_or(&[], Vec::as_slice)
    }
}

// The new side is the file text when we have it, so a string or comment that
// opens outside a hunk still colors the line. The old side is that file with
// the hunks reversed. Without the file, each hunk side is highlighted alone.
pub(super) fn captures(view: &DiffFileView, path: &str, old_path: Option<&str>) -> Syntax {
    if view.binary || view.truncated || view.too_large {
        return Syntax::default();
    }
    let mut syntax = Syntax::default();
    let old_path = old_path.unwrap_or(path);
    if let Some(file) = view.file.as_deref() {
        paint_document(&mut syntax, DiffSide::New, language_of(path), file);
        let old = old_text(file, &view.hunks);
        paint_document(&mut syntax, DiffSide::Old, language_of(old_path), &old);
    } else {
        paint_hunks(&mut syntax, DiffSide::New, language_of(path), &view.hunks);
        paint_hunks(
            &mut syntax,
            DiffSide::Old,
            language_of(old_path),
            &view.hunks,
        );
    }
    syntax
}

pub(super) fn line(
    text: &str,
    spans: &[(Range<usize>, String)],
    changed: Option<&Range<usize>>,
    tint: Option<Hsla>,
    theme: &HighlightTheme,
) -> StyledText {
    StyledText::new(text.to_owned()).with_highlights(runs(text, spans, changed, tint, theme))
}

fn runs(
    text: &str,
    spans: &[(Range<usize>, String)],
    changed: Option<&Range<usize>>,
    tint: Option<Hsla>,
    theme: &HighlightTheme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let syntax = resolve(text, spans, theme);
    match changed_word(text, changed, tint) {
        Some((range, background)) => with_background(text, &syntax, range, background),
        None => syntax,
    }
}

fn paint_document(syntax: &mut Syntax, side: DiffSide, language: &str, source: &str) {
    let lines = numbered(source);
    if lines.is_empty() {
        return;
    }
    place(syntax, side, &lines, &highlight(language, source));
}

fn paint_hunks(syntax: &mut Syntax, side: DiffSide, language: &str, hunks: &[DiffHunk]) {
    for hunk in hunks {
        let lines: Vec<&DiffLine> = hunk
            .lines
            .iter()
            .filter(|line| keeps(line.kind, side))
            .collect();
        paint_lines(syntax, side, language, &lines);
    }
}

fn paint_lines(syntax: &mut Syntax, side: DiffSide, language: &str, lines: &[&DiffLine]) {
    let mut source = String::new();
    let mut numbered = Vec::new();
    for line in lines {
        let Some(number) = line_number(line, side) else {
            continue;
        };
        if !source.is_empty() {
            source.push('\n');
        }
        let start = source.len();
        source.push_str(&line.text);
        if line.text.len() <= MAX_LINE {
            numbered.push((number, start..source.len()));
        }
    }
    if numbered.is_empty() {
        return;
    }
    place(syntax, side, &numbered, &highlight(language, &source));
}

fn keeps(kind: DiffLineKind, side: DiffSide) -> bool {
    !matches!(
        (side, kind),
        (DiffSide::Old, DiffLineKind::Add) | (DiffSide::New, DiffLineKind::Remove)
    )
}

fn line_number(line: &DiffLine, side: DiffSide) -> Option<u32> {
    match side {
        DiffSide::Old => line.old,
        DiffSide::New => line.new,
    }
}

fn numbered(source: &str) -> Vec<(u32, Range<usize>)> {
    let mut lines = Vec::new();
    for (index, range) in line_ranges(source).into_iter().enumerate() {
        if range.len() > MAX_LINE {
            continue;
        }
        let Ok(number) = u32::try_from(index + 1) else {
            continue;
        };
        lines.push((number, range));
    }
    lines
}

fn line_ranges(text: &str) -> Vec<Range<usize>> {
    let bytes = text.as_bytes();
    let mut ranges = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'\n' {
            index += 1;
            continue;
        }
        let end = if index > start && bytes[index - 1] == b'\r' {
            index - 1
        } else {
            index
        };
        ranges.push(start..end);
        index += 1;
        start = index;
    }
    if start < bytes.len() {
        ranges.push(start..bytes.len());
    }
    ranges
}

fn old_text(file: &str, hunks: &[DiffHunk]) -> String {
    let lines = line_ranges(file);
    let mut parts = Vec::new();
    let mut at = 0usize;
    for hunk in hunks {
        let start = usize::try_from(hunk.new_start.saturating_sub(1)).unwrap_or(at);
        let end = start.min(lines.len());
        for range in lines.get(at..end).unwrap_or_default() {
            parts.push(&file[range.clone()]);
        }
        at = at.max(end);
        for line in &hunk.lines {
            match line.kind {
                DiffLineKind::Remove => parts.push(&line.text),
                DiffLineKind::Add => at += 1,
                DiffLineKind::Context => {
                    parts.push(&line.text);
                    at += 1;
                }
            }
        }
    }
    for range in lines.get(at..).unwrap_or_default() {
        parts.push(&file[range.clone()]);
    }
    parts.join("\n")
}

fn place(
    syntax: &mut Syntax,
    side: DiffSide,
    lines: &[(u32, Range<usize>)],
    captures: &[(Range<usize>, String)],
) {
    for (capture, name) in captures {
        for (number, line) in lines {
            if line.end <= capture.start || line.start >= capture.end {
                continue;
            }
            let start = capture.start.max(line.start);
            let end = capture.end.min(line.end);
            if start >= end {
                continue;
            }
            syntax
                .lines
                .entry(key(side, *number))
                .or_default()
                .push((start - line.start..end - line.start, name.clone()));
        }
    }
}

fn key(side: DiffSide, line: u32) -> (u8, u32) {
    let side = match side {
        DiffSide::Old => 0,
        DiffSide::New => 1,
    };
    (side, line)
}

fn language_of(path: &str) -> &'static str {
    let name = path.rsplit(['/', '\\']).next().unwrap_or("");
    let name = name.to_ascii_lowercase();
    match name.as_str() {
        "makefile" | "gnumakefile" => return "make",
        "cmakelists.txt" => return "cmake",
        _ => {}
    }
    let Some(extension) = name.rsplit_once('.').map(|(_, extension)| extension) else {
        return "text";
    };
    extension_language(extension)
}

fn extension_language(extension: &str) -> &'static str {
    const LANGUAGES: &[(&str, &str)] = &[
        ("astro", "astro"),
        ("bash", "bash"),
        ("c", "c"),
        ("cc", "cpp"),
        ("cjs", "javascript"),
        ("cmake", "cmake"),
        ("cpp", "cpp"),
        ("cs", "csharp"),
        ("css", "css"),
        ("cts", "typescript"),
        ("cxx", "cpp"),
        ("diff", "diff"),
        ("ejs", "ejs"),
        ("erb", "erb"),
        ("ex", "elixir"),
        ("exs", "elixir"),
        ("go", "go"),
        ("gql", "graphql"),
        ("graphql", "graphql"),
        ("h", "c"),
        ("htm", "html"),
        ("html", "html"),
        ("hpp", "cpp"),
        ("hxx", "cpp"),
        ("java", "java"),
        ("js", "javascript"),
        ("json", "json"),
        ("jsonc", "json"),
        ("jsx", "javascript"),
        ("kt", "kotlin"),
        ("kts", "kotlin"),
        ("lua", "lua"),
        ("md", "markdown"),
        ("mdx", "markdown"),
        ("mjs", "javascript"),
        ("mts", "typescript"),
        ("php", "php"),
        ("php3", "php"),
        ("php4", "php"),
        ("php5", "php"),
        ("phtml", "php"),
        ("proto", "proto"),
        ("py", "python"),
        ("pyi", "python"),
        ("rb", "ruby"),
        ("rs", "rust"),
        ("scala", "scala"),
        ("scss", "css"),
        ("sh", "bash"),
        ("svelte", "svelte"),
        ("swift", "swift"),
        ("toml", "toml"),
        ("ts", "typescript"),
        ("tsx", "tsx"),
        ("yaml", "yaml"),
        ("yml", "yaml"),
        ("zig", "zig"),
    ];
    LANGUAGES
        .iter()
        .find(|(name, _)| *name == extension)
        .map_or("text", |(_, language)| *language)
}

fn highlight(language: &str, source: &str) -> Vec<(Range<usize>, String)> {
    if language == "text" || source.is_empty() {
        return Vec::new();
    }
    thread_local! {
        static CACHE: RefCell<HashMap<String, SyntaxHighlighter>> =
            RefCell::new(HashMap::new());
    }
    CACHE.with(|cache| highlight_cached(&mut cache.borrow_mut(), language, source))
}

fn highlight_cached(
    cache: &mut HashMap<String, SyntaxHighlighter>,
    language: &str,
    source: &str,
) -> Vec<(Range<usize>, String)> {
    let parsed = {
        let highlighter = cache
            .entry(language.to_owned())
            .or_insert_with(|| SyntaxHighlighter::new(language));
        if highlighter.language().as_ref() != language {
            return Vec::new();
        }
        let rope = Rope::from_str(source);
        let old_len = highlighter.text().len();
        let old_end = highlighter.text().offset_to_point(old_len);
        let new_end = rope.offset_to_point(rope.len());
        let edit = InputEdit {
            start_byte: 0,
            old_end_byte: old_len,
            new_end_byte: rope.len(),
            start_position: Point::new(0, 0),
            old_end_position: Point::new(old_end.row, old_end.column),
            new_end_position: Point::new(new_end.row, new_end.column),
        };
        highlighter.update(Some(edit), &rope, Some(PARSE_BUDGET))
    };
    if !parsed {
        cache.insert(language.to_owned(), SyntaxHighlighter::new(language));
        return Vec::new();
    }
    let Some(highlighter) = cache.get_mut(language) else {
        return Vec::new();
    };
    CAPTURES.with(|names| names.borrow_mut().clear());
    let styles = highlighter.styles(&(0..source.len()), &Recorded);
    let names = CAPTURES.with(|names| names.borrow().clone());
    names_of(&names, styles)
}

// `styles` merges overlaps and returns colors, not capture names. Each call
// records its name and hands back a color that only identifies that call.
// The merged run keeps the winning call's color. The resolver has to be
// `Sync`, so the names live in a thread local beside the highlighter cache.
thread_local! {
    static CAPTURES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

struct Recorded;

impl HighlightStyleResolver for Recorded {
    fn style(&self, name: &str) -> Option<HighlightStyle> {
        let color = CAPTURES.with(|names| {
            let mut names = names.borrow_mut();
            let color = marker(names.len())?;
            names.push(name.to_owned());
            Some(color)
        })?;
        Some(HighlightStyle {
            color: Some(color),
            ..Default::default()
        })
    }
}

fn names_of(
    names: &[String],
    styles: Vec<(Range<usize>, HighlightStyle)>,
) -> Vec<(Range<usize>, String)> {
    styles
        .into_iter()
        .filter_map(|(range, style)| Some((range, recorded_name(names, style.color?)?)))
        .collect()
}

#[allow(clippy::cast_precision_loss)]
fn marker(index: usize) -> Option<Hsla> {
    let hue = u32::try_from(index).ok()?;
    Some(Hsla {
        h: hue as f32,
        s: 0.,
        l: 0.,
        a: 0.,
    })
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn recorded_name(names: &[String], color: Hsla) -> Option<String> {
    let zero = 0.0_f32.to_bits();
    if color.s.to_bits() != zero || color.l.to_bits() != zero || color.a.to_bits() != zero {
        return None;
    }
    let index = color.h as usize;
    if (index as f32).to_bits() != color.h.to_bits() {
        return None;
    }
    names.get(index).cloned()
}

fn resolve(
    text: &str,
    spans: &[(Range<usize>, String)],
    theme: &HighlightTheme,
) -> Vec<(Range<usize>, HighlightStyle)> {
    spans
        .iter()
        .filter_map(|(range, name)| {
            let range = fit(text, range)?;
            theme.style(name).map(|style| (range, style))
        })
        .collect()
}

fn changed_word(
    text: &str,
    changed: Option<&Range<usize>>,
    tint: Option<Hsla>,
) -> Option<(Range<usize>, Hsla)> {
    let tint = tint?;
    let changed = fit(text, changed?)?;
    if text.get(changed.clone())?.trim().is_empty() {
        return None;
    }
    Some((changed, tint.opacity(0.30)))
}

fn with_background(
    text: &str,
    syntax: &[(Range<usize>, HighlightStyle)],
    changed: Range<usize>,
    background: Hsla,
) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut points = vec![0, text.len(), changed.start, changed.end];
    for (range, _) in syntax {
        points.push(range.start);
        points.push(range.end);
    }
    points.retain(|point| *point <= text.len());
    points.sort_unstable();
    points.dedup();
    let mut runs = Vec::new();
    for pair in points.windows(2) {
        let range = pair[0]..pair[1];
        if range.is_empty()
            || !text.is_char_boundary(range.start)
            || !text.is_char_boundary(range.end)
        {
            continue;
        }
        let mut style = syntax
            .iter()
            .find(|(span, _)| span.start <= range.start && range.end <= span.end)
            .map(|(_, style)| *style)
            .unwrap_or_default();
        if changed.start <= range.start && range.end <= changed.end {
            style.background_color = Some(background);
        }
        if style != HighlightStyle::default() {
            runs.push((range, style));
        }
    }
    runs
}

fn fit(text: &str, range: &Range<usize>) -> Option<Range<usize>> {
    if range.end > text.len()
        || range.start >= range.end
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return None;
    }
    Some(range.clone())
}

#[cfg(test)]
mod tests;
