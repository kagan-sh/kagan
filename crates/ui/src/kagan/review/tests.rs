use kg_domain::{DiffFileView, DiffHunk, DiffLine, DiffLineKind, DiffSide};

use super::rows::{ReviewRow, review_rows};

use crate::kagan::Kagan;

pub(crate) fn review_note_count(kagan: &Kagan) -> usize {
    kagan
        .screen
        .review()
        .map_or(0, |review| kagan.review_drafts.count(&review.thread))
}

pub(crate) fn review_matched_text(kagan: &Kagan) -> Option<String> {
    let review = kagan.screen.review()?;
    match review.rows.get(review.match_row?)? {
        ReviewRow::Line { line, .. } => Some(line.text.clone()),
        ReviewRow::Fold { lines } => Some(
            lines
                .iter()
                .map(|line| line.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        ),
    }
}

pub(crate) fn review_horizontal_limit(kagan: &Kagan) -> Option<f32> {
    kagan
        .screen
        .review()
        .map(|review| review.scroll.max_offset().x.as_f32())
}

pub(crate) fn review_horizontal_offset(kagan: &Kagan) -> Option<f32> {
    kagan
        .screen
        .review()
        .map(|review| review.scroll.offset().x.as_f32())
}

pub(crate) fn review_token(
    kagan: &Kagan,
    side: DiffSide,
    line: u32,
    byte: usize,
) -> Option<String> {
    let review = kagan.screen.review()?;
    review
        .syntax
        .spans(side, line)
        .iter()
        .find(|(range, _)| range.contains(&byte))
        .map(|(_, name)| name.clone())
}

pub(crate) fn open_diff_lines(kagan: &Kagan) -> Vec<String> {
    kagan
        .screen
        .review()
        .map(|review| review.shown.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}

#[test]
fn folds_unchanged_lines_and_marks_changed_words() {
    let line = |kind, old, new, text: &str| DiffLine {
        kind,
        old,
        new,
        text: text.into(),
    };
    let view = DiffFileView {
        path: "a".into(),
        revision: String::new(),
        binary: false,
        truncated: false,
        too_large: false,
        file: Some("first\nnew word\nlast\n".into()),
        added: 1,
        removed: 1,
        hunks: vec![DiffHunk {
            old_start: 2,
            new_start: 2,
            lines: vec![
                line(DiffLineKind::Remove, Some(2), None, "old word"),
                line(DiffLineKind::Add, None, Some(2), "new word"),
            ],
        }],
    };
    let rows = review_rows(&view);
    assert!(
        matches!(&rows[0], ReviewRow::Fold { lines } if lines[0].old == Some(1) && lines[0].new == Some(1))
    );
    assert!(matches!(&rows[1], ReviewRow::Line { changed: Some(range), .. } if range == &(0..3)));
    assert!(matches!(&rows[2], ReviewRow::Line { changed: Some(range), .. } if range == &(0..3)));
    assert!(
        matches!(&rows[3], ReviewRow::Fold { lines } if lines[0].old == Some(3) && lines[0].new == Some(3))
    );
}

#[test]
fn pairs_each_replacement_and_leaves_unmatched_lines_unmarked() {
    let line = |kind, text: &str| DiffLine {
        kind,
        old: None,
        new: None,
        text: text.into(),
    };
    let view = DiffFileView {
        path: "a".into(),
        revision: String::new(),
        binary: false,
        truncated: false,
        too_large: false,
        file: None,
        added: 2,
        removed: 3,
        hunks: vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                line(DiffLineKind::Remove, "old one"),
                line(DiffLineKind::Remove, "old two"),
                line(DiffLineKind::Remove, "extra"),
                line(DiffLineKind::Add, "new one"),
                line(DiffLineKind::Add, "new two"),
            ],
        }],
    };
    let rows = review_rows(&view);
    assert!(matches!(&rows[0], ReviewRow::Line { changed: Some(range), .. } if range == &(0..3)));
    assert!(matches!(&rows[1], ReviewRow::Line { changed: Some(range), .. } if range == &(0..3)));
    assert!(matches!(&rows[2], ReviewRow::Line { changed: None, .. }));
    assert!(matches!(&rows[3], ReviewRow::Line { changed: Some(range), .. } if range == &(0..3)));
    assert!(matches!(&rows[4], ReviewRow::Line { changed: Some(range), .. } if range == &(0..3)));
}

#[test]
fn highlights_only_inserted_spaces_at_word_boundaries() {
    let line = |kind, text: &str| DiffLine {
        kind,
        old: None,
        new: None,
        text: text.into(),
    };
    let view = DiffFileView {
        path: "a".into(),
        revision: String::new(),
        binary: false,
        truncated: false,
        too_large: false,
        file: None,
        added: 1,
        removed: 1,
        hunks: vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                line(DiffLineKind::Remove, "get List"),
                line(DiffLineKind::Add, "get   List"),
            ],
        }],
    };
    let rows = review_rows(&view);
    assert!(matches!(&rows[0], ReviewRow::Line { changed: Some(range), .. } if range.is_empty()));
    assert!(matches!(&rows[1], ReviewRow::Line { changed: Some(range), .. } if range == &(4..6)));
}

#[test]
fn leaves_unrelated_replacements_unmarked() {
    let line = |kind, text: &str| DiffLine {
        kind,
        old: None,
        new: None,
        text: text.into(),
    };
    let view = DiffFileView {
        path: "a".into(),
        revision: String::new(),
        binary: false,
        truncated: false,
        too_large: false,
        file: None,
        added: 1,
        removed: 1,
        hunks: vec![DiffHunk {
            old_start: 1,
            new_start: 1,
            lines: vec![
                line(
                    DiffLineKind::Remove,
                    "  help  Print this message or the help of the given subcommand(s)",
                ),
                line(
                    DiffLineKind::Add,
                    "  remove  Remove a project and its threads",
                ),
            ],
        }],
    };
    let rows = review_rows(&view);
    assert!(matches!(&rows[0], ReviewRow::Line { changed: None, .. }));
    assert!(matches!(&rows[1], ReviewRow::Line { changed: None, .. }));
}
