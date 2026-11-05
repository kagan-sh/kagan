use kg_domain::{Event, EventKind, Order, ProjectId, ThreadId, Timestamp};
use rusqlite::{Connection, OptionalExtension as _, Transaction, params};

use crate::Result;
use crate::column::{Json, Text};

impl super::Store {
    pub fn last_started(&self, id: &ThreadId) -> Result<Option<Timestamp>> {
        Ok(self
            .conn
            .query_row(
                "SELECT at FROM events WHERE thread_id = ?1
                 AND json_extract(kind, '$.type') = 'started' ORDER BY seq DESC LIMIT 1",
                [Text(id)],
                |row| row.get(0).map(Timestamp),
            )
            .optional()?)
    }

    /// Appends in order after the thread's last event, numbering from 1.
    pub fn append_events(
        &mut self,
        thread_id: &ThreadId,
        at: Timestamp,
        kinds: impl IntoIterator<Item = EventKind>,
    ) -> Result<Vec<Event>> {
        let tx = self.conn.transaction()?;
        let events = append_events(&tx, thread_id, at, kinds)?;
        tx.commit()?;
        Ok(events)
    }

    pub fn events_after(
        &self,
        thread_id: &ThreadId,
        after_seq: Option<u64>,
        limit: Option<u32>,
        order: Order,
    ) -> Result<Vec<Event>> {
        let direction = match order {
            Order::Asc => "ASC",
            Order::Desc => "DESC",
        };
        let mut statement = self.conn.prepare(&format!(
            "SELECT thread_id, seq, at, kind FROM current_events
             WHERE thread_id = ?1 AND seq > ?2 ORDER BY seq {direction} LIMIT ?3"
        ))?;
        let events = statement
            .query_map(
                params![
                    Text(thread_id),
                    after_seq.unwrap_or(0),
                    limit.map_or(-1, i64::from)
                ],
                event_row,
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Newest ending events per thread, four at most, newest first.
    pub fn endings(&self) -> Result<Vec<(ThreadId, Vec<EventKind>)>> {
        let mut statement = self.conn.prepare(
            "SELECT thread_id, kind FROM ( \
                SELECT thread_id, kind, \
                    ROW_NUMBER() OVER (PARTITION BY thread_id ORDER BY seq DESC) AS n \
                FROM events \
                WHERE json_extract(kind, '$.type') IN ('stopped', 'exited', 'failed') \
             ) WHERE n <= 4 ORDER BY thread_id, n",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, Text<_>>(0)?.0,
                row.get::<_, Json<EventKind>>(1)?.0,
            ))
        })?;
        let mut grouped: Vec<(ThreadId, Vec<EventKind>)> = Vec::new();
        for row in rows {
            let (id, kind) = row?;
            if let Some((last, kinds)) = grouped.last_mut()
                && *last == id
            {
                kinds.push(kind);
            } else {
                grouped.push((id, vec![kind]));
            }
        }
        Ok(grouped)
    }
}

pub(super) fn append_deleted(
    tx: &Transaction,
    ids: &[ThreadId],
    at: Timestamp,
) -> Result<Vec<(Event, ProjectId)>> {
    let mut events = Vec::new();
    for id in ids {
        let project_id: ProjectId = tx.query_row(
            "SELECT project_id FROM threads WHERE id = ?1",
            [Text(id)],
            |row| Ok(row.get::<_, Text<_>>(0)?.0),
        )?;
        events.extend(
            append_events(tx, id, at, [EventKind::Deleted])?
                .into_iter()
                .map(|event| (event, project_id.clone())),
        );
    }
    Ok(events)
}

pub(super) fn append_events(
    conn: &Connection,
    thread_id: &ThreadId,
    at: Timestamp,
    kinds: impl IntoIterator<Item = EventKind>,
) -> Result<Vec<Event>> {
    let last: u64 = conn.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM events WHERE thread_id = ?1",
        [Text(thread_id)],
        |row| row.get(0),
    )?;
    let mut insert =
        conn.prepare("INSERT INTO events (thread_id, seq, at, kind) VALUES (?1, ?2, ?3, ?4)")?;
    let events = (last + 1..)
        .zip(kinds)
        .map(|(seq, kind)| {
            insert.execute(params![Text(thread_id), seq, at.0, Json(&kind)])?;
            Ok(Event {
                thread_id: thread_id.clone(),
                seq,
                at,
                kind,
            })
        })
        .collect::<Result<_>>()?;
    drop(insert);
    Ok(events)
}

fn event_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Event> {
    Ok(Event {
        thread_id: row.get::<_, Text<_>>(0)?.0,
        seq: row.get(1)?,
        at: Timestamp(row.get(2)?),
        kind: row.get::<_, Json<_>>(3)?.0,
    })
}
