pub(super) const ROW_LIMIT: usize = 100;
pub(super) const COLUMN_LIMIT: usize = 20;
pub(super) const CELL_CHARS: usize = 120;

pub(super) struct Table {
    pub(super) rows: Vec<Vec<String>>,
    pub(super) rows_total: usize,
    pub(super) columns_total: usize,
    pub(super) truncated: bool,
}

pub(super) fn parse(text: &str, delimiter: char) -> Table {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut cell = String::new();
    let mut cell_chars = 0;
    let mut quoted = false;
    let mut started = false;
    let mut rows_total = 0;
    let mut columns_total = 0;
    let mut columns = 0;
    let mut chars = text.chars().peekable();
    while let Some(ch) = chars.next() {
        if quoted {
            started = true;
            if ch != '"' {
                push_cell_char(&mut cell, &mut cell_chars, ch);
            } else if chars.peek() == Some(&'"') {
                chars.next();
                push_cell_char(&mut cell, &mut cell_chars, '"');
            } else {
                quoted = false;
            }
            continue;
        }
        if ch == '"' && cell.is_empty() {
            quoted = true;
            started = true;
        } else if ch == delimiter {
            push_cell(&mut cell, &mut cell_chars, &mut row, &mut columns);
            started = true;
        } else if ch == '\n' {
            push_cell(&mut cell, &mut cell_chars, &mut row, &mut columns);
            end_row(
                &mut rows,
                &mut row,
                &mut rows_total,
                &mut columns,
                &mut columns_total,
            );
            started = false;
        } else if ch != '\r' {
            push_cell_char(&mut cell, &mut cell_chars, ch);
            started = true;
        }
    }
    if started {
        push_cell(&mut cell, &mut cell_chars, &mut row, &mut columns);
        end_row(
            &mut rows,
            &mut row,
            &mut rows_total,
            &mut columns,
            &mut columns_total,
        );
    }
    Table {
        rows,
        rows_total,
        columns_total,
        truncated: rows_total > ROW_LIMIT || columns_total > COLUMN_LIMIT,
    }
}

fn push_cell_char(cell: &mut String, chars: &mut usize, ch: char) {
    if *chars < CELL_CHARS {
        cell.push(if ch == '\n' || ch == '\r' { ' ' } else { ch });
        *chars += 1;
    }
}

fn push_cell(
    cell: &mut String,
    cell_chars: &mut usize,
    row: &mut Vec<String>,
    columns: &mut usize,
) {
    *columns += 1;
    if *columns <= COLUMN_LIMIT {
        row.push(std::mem::take(cell));
    } else {
        cell.clear();
    }
    *cell_chars = 0;
}

fn end_row(
    rows: &mut Vec<Vec<String>>,
    row: &mut Vec<String>,
    rows_total: &mut usize,
    columns: &mut usize,
    columns_total: &mut usize,
) {
    *rows_total += 1;
    *columns_total = (*columns_total).max(*columns);
    if *rows_total <= ROW_LIMIT {
        rows.push(std::mem::take(row));
    } else {
        row.clear();
    }
    *columns = 0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_hide_delimiters_and_escaped_quotes() {
        let table = parse("a,b\n\"x,y\",\"say \"\"hi\"\"\"\n", ',');
        assert_eq!(table.rows[1], vec!["x,y", "say \"hi\""]);
        assert_eq!(table.rows_total, 2);
    }

    #[test]
    fn a_quoted_newline_stays_in_the_cell_and_crlf_is_dropped() {
        let table = parse("a,b\r\n\"one\r\ntwo\",x\r\n", ',');
        assert_eq!(table.rows[1], vec!["one  two", "x"]);
        assert_eq!(table.rows_total, 2);
    }

    #[test]
    fn a_ragged_row_keeps_its_own_width() {
        let table = parse("a\na,b,c\n", ',');
        assert_eq!(table.rows[0], vec!["a"]);
        assert_eq!(table.rows[1], vec!["a", "b", "c"]);
        assert_eq!(table.columns_total, 3);
    }

    #[test]
    fn a_cell_stops_at_the_character_cap() {
        let long = "x".repeat(CELL_CHARS + 10);
        let table = parse(&format!("{long},{long}\n"), ',');
        assert_eq!(table.rows[0][0].chars().count(), CELL_CHARS);
        assert_eq!(table.rows[0][1].chars().count(), CELL_CHARS);
    }

    #[test]
    fn rows_and_columns_stop_at_their_caps() {
        let wide = (0..COLUMN_LIMIT + 5)
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",");
        let mut text = String::new();
        for _ in 0..=ROW_LIMIT {
            text.push_str(&wide);
            text.push('\n');
        }
        let table = parse(&text, ',');
        assert_eq!(table.rows.len(), ROW_LIMIT);
        assert_eq!(table.rows[0].len(), COLUMN_LIMIT);
        assert_eq!(table.rows_total, ROW_LIMIT + 1);
        assert_eq!(table.columns_total, COLUMN_LIMIT + 5);
    }
}
