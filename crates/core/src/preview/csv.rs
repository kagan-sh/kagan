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
                push_cell_char(&mut cell, ch);
            } else if chars.peek() == Some(&'"') {
                chars.next();
                push_cell_char(&mut cell, '"');
            } else {
                quoted = false;
            }
            continue;
        }
        if ch == '"' && cell.is_empty() {
            quoted = true;
            started = true;
        } else if ch == delimiter {
            push_cell(&mut cell, &mut row, &mut columns);
            started = true;
        } else if ch == '\n' {
            push_cell(&mut cell, &mut row, &mut columns);
            end_row(
                &mut rows,
                &mut row,
                &mut rows_total,
                &mut columns,
                &mut columns_total,
            );
            started = false;
        } else if ch != '\r' {
            push_cell_char(&mut cell, ch);
            started = true;
        }
    }
    if started {
        push_cell(&mut cell, &mut row, &mut columns);
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

fn push_cell_char(cell: &mut String, ch: char) {
    if cell.chars().count() < CELL_CHARS {
        cell.push(if ch == '\n' || ch == '\r' { ' ' } else { ch });
    }
}

fn push_cell(cell: &mut String, row: &mut Vec<String>, columns: &mut usize) {
    *columns += 1;
    if *columns <= COLUMN_LIMIT {
        row.push(std::mem::take(cell));
    } else {
        cell.clear();
    }
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
