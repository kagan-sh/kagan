use kg_domain::{SCREEN_MAX_BYTES, TERMINAL_COLS, TERMINAL_ROWS, ThreadId};
use rusqlite::{OptionalExtension, Row, params};

use crate::column::Text;
use crate::{Error, Result};

/// `started_seq` is the `Started` event of the run that produced the screen. `replay` is empty
/// in the rows from [`super::Store::screens`]; [`super::Store::screen_replay`] reads it.
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
        if !screen_in_bounds(row.cols, row.rows, row.replay.len(), row.screen_kept) {
            return Err(Error::Conflict(
                "the screen snapshot is outside the geometry or the 4 MiB bound".into(),
            ));
        }
        let transaction = self.conn.unchecked_transaction()?;
        let latest: Option<u64> = transaction.query_row(
            &format!("SELECT {}", super::latest_start("?1")),
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
                row.cols,
                row.rows,
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

    /// Drops the screens of every thread archived at or before `archived_before`.
    pub fn delete_archived_screens(&self, archived_before: i64) -> Result<()> {
        self.conn.execute(
            "DELETE FROM terminal_screens WHERE thread_id IN \
             (SELECT id FROM threads WHERE archived_at <= ?1)",
            [archived_before],
        )?;
        Ok(())
    }

    /// Screens whose run is still the latest `Started`, without their replay. A row outside the
    /// bound comes back with `screen_kept` clear, so a reader never builds a terminal from it.
    pub fn screens(&self) -> Result<Vec<ScreenRow>> {
        let mut statement = self.conn.prepare(&format!(
            "SELECT thread_id, started_seq, cols, rows, length(replay) AS replay_len, \
                history_dropped, screen_kept \
             FROM terminal_screens WHERE started_seq = {}",
            super::latest_start("terminal_screens.thread_id")
        ))?;
        let rows = statement.query_map([], screen_row)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Error::from)
    }

    /// The replay of the thread's latest run, or none when no screen is saved for it.
    pub fn screen_replay(&self, id: &ThreadId) -> Result<Option<Vec<u8>>> {
        Ok(self
            .conn
            .query_row(
                &format!(
                    "SELECT replay FROM terminal_screens WHERE thread_id = ?1 AND started_seq = {}",
                    super::latest_start("terminal_screens.thread_id")
                ),
                [Text(id)],
                |row| row.get(0),
            )
            .optional()?)
    }
}

fn screen_in_bounds(cols: u16, rows: u16, replay_len: usize, screen_kept: bool) -> bool {
    TERMINAL_COLS.contains(&cols)
        && TERMINAL_ROWS.contains(&rows)
        && replay_len <= SCREEN_MAX_BYTES
        && (screen_kept || replay_len == 0)
}

fn screen_row(row: &Row<'_>) -> rusqlite::Result<ScreenRow> {
    let thread_id = row.get::<_, Text<_>>("thread_id")?.0;
    let started_seq = row.get("started_seq")?;
    let cols = u16::try_from(row.get::<_, i64>("cols")?).unwrap_or(0);
    let rows = u16::try_from(row.get::<_, i64>("rows")?).unwrap_or(0);
    let replay_len = usize::try_from(row.get::<_, i64>("replay_len")?).unwrap_or(usize::MAX);
    let screen_kept: bool = row.get("screen_kept")?;
    if !screen_in_bounds(cols, rows, replay_len, screen_kept) {
        return Ok(ScreenRow {
            thread_id,
            started_seq,
            cols: 80,
            rows: 24,
            replay: Vec::new(),
            history_dropped: 0,
            screen_kept: false,
        });
    }
    let history_dropped = if screen_kept {
        u32::try_from(row.get::<_, i64>("history_dropped")?).unwrap_or(u32::MAX)
    } else {
        0
    };
    Ok(ScreenRow {
        thread_id,
        started_seq,
        cols,
        rows,
        replay: Vec::new(),
        history_dropped,
        screen_kept,
    })
}
