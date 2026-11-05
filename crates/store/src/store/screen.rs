use kg_domain::ThreadId;
use rusqlite::{Row, params};

use crate::column::Text;
use crate::{Error, Result};

// Matches `kg_terminal::SNAPSHOT_MAX`. History and the visible redraw share this bound.
const SCREEN_MAX_BYTES: usize = 4 * 1024 * 1024;
const SCREEN_MIN_COLS: i64 = 2;
const SCREEN_MAX_COLS: i64 = 1000;
const SCREEN_MIN_ROWS: i64 = 1;
const SCREEN_MAX_ROWS: i64 = 500;

/// `started_seq` is the `Started` event of the run that produced the screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScreenRow {
    pub thread_id: ThreadId,
    pub started_seq: u64,
    pub cols: u16,
    pub rows: u16,
    pub replay: Vec<u8>,
    pub history_dropped: u32,
    pub screen_kept: bool,
}

impl super::Store {
    /// Writes the screen only when `started_seq` is still that thread's latest `Started` event.
    /// A delayed snapshot from an older run returns `Ok(false)` and changes nothing.
    pub fn put_screen(&self, row: &ScreenRow) -> Result<bool> {
        let cols = i64::from(row.cols);
        let rows = i64::from(row.rows);
        if !screen_in_bounds(cols, rows, row.replay.len(), row.screen_kept) {
            return Err(Error::Conflict(
                "the screen snapshot is outside the geometry or the 4 MiB bound".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let latest: Option<u64> = transaction.query_row(
            "SELECT MAX(seq) FROM events \
             WHERE thread_id = ?1 AND json_extract(kind, '$.type') = 'started'",
            [Text(&row.thread_id)],
            |row| row.get(0),
        )?;
        if latest != Some(row.started_seq) {
            transaction.commit()?;
            return Ok(false);
        }
        transaction.execute(
            "INSERT INTO terminal_screens \
                (thread_id, started_seq, cols, rows, replay, history_dropped, screen_kept) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) \
             ON CONFLICT(thread_id) DO UPDATE SET \
                started_seq = excluded.started_seq, cols = excluded.cols, rows = excluded.rows, \
                replay = excluded.replay, history_dropped = excluded.history_dropped, \
                screen_kept = excluded.screen_kept",
            params![
                Text(&row.thread_id),
                row.started_seq,
                cols,
                rows,
                row.replay,
                row.history_dropped,
                i64::from(u8::from(row.screen_kept)),
            ],
        )?;
        transaction.commit()?;
        Ok(true)
    }

    pub fn delete_screen(&self, id: &ThreadId) -> Result<()> {
        self.conn.execute(
            "DELETE FROM terminal_screens WHERE thread_id = ?1",
            [Text(id)],
        )?;
        Ok(())
    }

    /// Screens whose run is still the latest `Started`. A row outside the bound comes back with an
    /// empty replay and `screen_kept` clear, so a reader never builds a terminal from it.
    pub fn screens(&self) -> Result<Vec<ScreenRow>> {
        let mut statement = self.conn.prepare(
            "SELECT thread_id, started_seq, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 \
                          AND ((screen_kept = 0 AND length(replay) = 0) OR screen_kept != 0) \
                     THEN cols ELSE 80 END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 \
                          AND ((screen_kept = 0 AND length(replay) = 0) OR screen_kept != 0) \
                     THEN rows ELSE 24 END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 \
                          AND ((screen_kept = 0 AND length(replay) = 0) OR screen_kept != 0) \
                     THEN replay ELSE X'' END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 AND screen_kept != 0 \
                     THEN history_dropped ELSE 0 END, \
                CASE WHEN cols BETWEEN ?1 AND ?2 AND rows BETWEEN ?3 AND ?4 \
                          AND length(replay) <= ?5 AND screen_kept != 0 \
                     THEN 1 ELSE 0 END \
             FROM terminal_screens \
             WHERE started_seq = ( \
                    SELECT MAX(seq) FROM events \
                    WHERE events.thread_id = terminal_screens.thread_id \
                      AND json_extract(kind, '$.type') = 'started')",
        )?;
        let rows = statement.query_map(
            params![
                SCREEN_MIN_COLS,
                SCREEN_MAX_COLS,
                SCREEN_MIN_ROWS,
                SCREEN_MAX_ROWS,
                i64::try_from(SCREEN_MAX_BYTES).unwrap_or(i64::MAX),
            ],
            screen_row,
        )?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Error::from)
    }
}

fn screen_in_bounds(cols: i64, rows: i64, replay_len: usize, screen_kept: bool) -> bool {
    (SCREEN_MIN_COLS..=SCREEN_MAX_COLS).contains(&cols)
        && (SCREEN_MIN_ROWS..=SCREEN_MAX_ROWS).contains(&rows)
        && replay_len <= SCREEN_MAX_BYTES
        && (screen_kept || replay_len == 0)
}

fn screen_row(row: &Row<'_>) -> rusqlite::Result<ScreenRow> {
    let cols: i64 = row.get(2)?;
    let rows: i64 = row.get(3)?;
    let replay: Vec<u8> = row.get(4)?;
    let kept: i64 = row.get(6)?;
    let screen_kept = kept != 0;
    if !screen_in_bounds(cols, rows, replay.len(), screen_kept) {
        return Err(rusqlite::Error::InvalidColumnType(
            4,
            "replay".into(),
            rusqlite::types::Type::Blob,
        ));
    }
    let cols = u16::try_from(cols).map_err(|_| {
        rusqlite::Error::InvalidColumnType(2, "cols".into(), rusqlite::types::Type::Integer)
    })?;
    let rows = u16::try_from(rows).map_err(|_| {
        rusqlite::Error::InvalidColumnType(3, "rows".into(), rusqlite::types::Type::Integer)
    })?;
    let history_dropped = u32::try_from(row.get::<_, i64>(5)?).unwrap_or(u32::MAX);
    Ok(ScreenRow {
        thread_id: row.get::<_, Text<_>>(0)?.0,
        started_seq: row.get(1)?,
        cols,
        rows,
        replay,
        history_dropped,
        screen_kept,
    })
}
