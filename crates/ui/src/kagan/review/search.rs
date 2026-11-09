use super::Kagan;
use super::rows::ReviewRow;
use gpui_kit::{Context, Focusable as _, Window};

impl Kagan {
    pub(super) fn find_in_review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(review) = self.screen.review_mut() {
            review.searching = Some(());
            window.focus(&review.search.focus_handle(cx), cx);
            cx.notify();
        }
    }

    pub(super) fn search_match(&mut self, next: bool, cx: &mut Context<Self>) {
        let Some(review) = self.screen.review_mut() else {
            return;
        };
        let query = review.search.read(cx).value().to_lowercase();
        if query.is_empty() {
            return;
        }
        let matches: Vec<_> = review
            .rows
            .iter()
            .enumerate()
            .filter_map(|(index, row)| {
                let found = match row {
                    ReviewRow::Line { line, .. } => line.text.to_lowercase().contains(&query),
                    ReviewRow::Fold { lines } => lines
                        .iter()
                        .any(|line| line.text.to_lowercase().contains(&query)),
                };
                found.then_some(index)
            })
            .collect();
        if matches.is_empty() {
            review.match_at = None;
            cx.notify();
            return;
        }
        let at = review.match_at.map_or(0, |at| {
            if next {
                (at + 1) % matches.len()
            } else {
                (at + matches.len() - 1) % matches.len()
            }
        });
        let row = matches[at];
        review.match_at = Some(at);
        review.match_row = Some(row);
        if let Some(ReviewRow::Fold { lines }) = review.rows.get(row)
            && let Some(line) = lines.first().and_then(|line| line.new.or(line.old))
        {
            review.expanded.insert(line);
        }
        review.scroll.scroll_to_top_of_item(row);
        cx.notify();
    }
}
