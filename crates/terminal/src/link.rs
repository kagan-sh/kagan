use std::ops::Range;

use crate::Grid;

/// The http and https links in `text`, as byte ranges. A link stops at whitespace, a quote or an
/// angle bracket, and leaves out trailing punctuation and closing brackets it did not open.
#[must_use]
pub fn links(text: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = scheme(&text[from..]).map(|at| from + at) {
        let rest = &text[start..];
        let length = rest
            .find(|c: char| c.is_whitespace() || c.is_control() || "\"'<>`".contains(c))
            .unwrap_or(rest.len());
        let end = start + trimmed(&rest[..length]);
        if text[start..end]
            .split_once("://")
            .is_some_and(|(_, host)| !host.is_empty())
        {
            found.push(start..end);
        }
        from = end.max(start + 1);
    }
    found
}

fn scheme(text: &str) -> Option<usize> {
    [text.find("http://"), text.find("https://")]
        .into_iter()
        .flatten()
        .min()
}

fn trimmed(link: &str) -> usize {
    let mut end = link.len();
    while let Some(last) = link[..end].chars().next_back() {
        let open = match last {
            ')' => '(',
            ']' => '[',
            '}' => '{',
            '.' | ',' | ':' | ';' | '!' | '?' => {
                end -= 1;
                continue;
            }
            _ => break,
        };
        let text = &link[..end];
        if text.matches(open).count() >= text.matches(last).count() {
            break;
        }
        end -= 1;
    }
    end
}

fn web(uri: &str) -> bool {
    uri.starts_with("http://") || uri.starts_with("https://")
}

impl Grid {
    /// The http or https link under a cell: the program's own hyperlink there, or a link in the
    /// text, followed across rows the terminal wrapped.
    #[must_use]
    pub fn link_at(&self, line: usize, column: usize) -> Option<String> {
        if let Some((_, _, uri)) = self
            .hyperlinks
            .iter()
            .find(|(row, columns, _)| *row == line && columns.contains(&column))
        {
            return web(uri).then(|| uri.clone());
        }
        let wraps = |row: usize| self.wrapped.get(row).copied().unwrap_or(false);
        let mut row = line;
        while row > 0 && wraps(row - 1) {
            row -= 1;
        }
        let mut text = String::new();
        let mut at = None;
        loop {
            for (index, cell) in self.lines.get(row)?.iter().enumerate() {
                if (row, index) == (line, column) {
                    at = Some(text.len());
                }
                text.push(cell.c);
            }
            if !wraps(row) {
                break;
            }
            row += 1;
        }
        let at = at?;
        links(&text)
            .into_iter()
            .find(|range| range.contains(&at))
            .map(|range| text[range].to_owned())
    }
}

#[cfg(test)]
mod tests;
