use super::*;

pub(crate) fn review_send_state(kagan: &Kagan) -> Option<(bool, Option<String>)> {
    kagan
        .screen
        .review()
        .map(|review| (review.sending, review.send_error.clone()))
}

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

pub(crate) fn open_diff_lines(kagan: &Kagan) -> Vec<String> {
    kagan
        .screen
        .review()
        .map(|review| review.shown.lines().map(str::to_owned).collect())
        .unwrap_or_default()
}
