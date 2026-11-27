use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use kg_domain::{
    Event, EventKind, PendingInteractionId, PlanEntry, Role, ThreadOption, Timestamp, ToolCall,
    ToolDiff, ToolKind, ToolStatus, TurnOutcome, WorkKind,
};

use super::kind;

// id is the seq of the first event the row came from, so it is stable across appends.
#[derive(Clone, Debug)]
pub(super) struct Row {
    pub(super) id: u64,
    pub(super) kind: RowKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum RowKey {
    Row(u64),
    Bundle(u64),
}

impl Row {
    pub(super) fn key(&self) -> RowKey {
        if matches!(self.kind, RowKind::Bundle(_)) {
            RowKey::Bundle(self.id)
        } else {
            RowKey::Row(self.id)
        }
    }
}

#[derive(Clone, Debug)]
pub(super) enum RowKind {
    Message {
        role: Role,
        text: String,
        message_id: Option<String>,
        at: Timestamp,
    },
    Thought(Box<ThoughtRow>),
    Work(Box<ToolRow>),
    Bundle(Vec<Row>),
    Notice(String),
    Turn(Box<TurnRow>),
}

#[derive(Clone, Debug)]
pub(super) struct TurnRow {
    pub(super) from: Timestamp,
    pub(super) to: Timestamp,
    pub(super) outcome: TurnOutcome,
    pub(super) members: Vec<Row>,
}

#[derive(Clone, Debug)]
pub(super) struct ThoughtRow {
    pub(super) text: String,
    pub(super) message_id: Option<String>,
    pub(super) from: Timestamp,
    pub(super) to: Timestamp,
}

#[derive(Clone, Debug)]
pub(super) struct ToolRow {
    pub(super) call: ToolCall,
    pub(super) counts: (usize, usize),
    pub(super) from: Timestamp,
    pub(super) to: Timestamp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolStage {
    Waiting,
    Running,
    Settled,
}

impl ToolRow {
    pub(super) fn stage(&self) -> ToolStage {
        match self.call.status {
            Some(ToolStatus::Pending) => ToolStage::Waiting,
            Some(ToolStatus::InProgress) => ToolStage::Running,
            _ => ToolStage::Settled,
        }
    }

    pub(super) fn running(&self) -> bool {
        self.stage() != ToolStage::Settled
    }

    pub(super) fn failed(&self) -> bool {
        self.call.status == Some(ToolStatus::Failed)
    }

    pub(super) fn live_body(&self) -> bool {
        self.running() && self.call.output.is_some() && self.call.kind != Some(ToolKind::Execute)
    }

    pub(super) fn folds(&self) -> bool {
        self.call.status == Some(ToolStatus::Completed)
            && matches!(
                kind::of(self),
                WorkKind::Read | WorkKind::Search | WorkKind::Fetch
            )
    }

    pub(super) fn span(&self) -> Option<Duration> {
        if self.stage() == ToolStage::Waiting {
            return None;
        }
        let span = super::kind::span_of(self.from, self.to);
        (!span.is_zero()).then_some(span)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fold {
    Ignored,
    Dirty(usize),
}

#[derive(Default)]
pub(super) struct Transcript {
    // Rows are shared with the renderer, so a frame never copies a body's output or a diff.
    pub(super) rows: Vec<Rc<Row>>,
    pub(super) usage: Option<(u64, u64)>,
    // The modes and options the agent offers, whole; the box's chip and its menu read this.
    pub(super) options: Vec<ThreadOption>,
    pub(super) last_outcome: Option<TurnOutcome>,
    open: BTreeMap<RowKey, bool>,
    last_seq: Option<u64>,
    turn: Option<(u64, Timestamp)>,
    // The plan card over the box reads the turn's latest plan; the transcript draws no row for it.
    plan: Option<(u64, Vec<PlanEntry>)>,
}

impl Transcript {
    pub(super) fn absorb(&mut self, event: &Event) -> Fold {
        if self.last_seq.is_some_and(|seq| event.seq <= seq) {
            return Fold::Ignored;
        }
        self.last_seq = Some(event.seq);
        match &event.kind {
            EventKind::Message {
                role,
                text,
                message_id,
                ..
            } => self.chunk(*role, text, message_id.clone(), event, false),
            EventKind::Thought { text, message_id } => {
                self.chunk(Role::Agent, text, message_id.clone(), event, true)
            }
            EventKind::Tool(call) => {
                let delta = ToolRow {
                    call: call.clone(),
                    counts: diff_counts(&call.diff),
                    from: event.at,
                    to: event.at,
                };
                self.tool(event.seq, delta)
            }
            EventKind::Plan { entries } => {
                let id = self.plan.as_ref().map_or(event.seq, |(id, _)| *id);
                self.plan = Some((id, entries.clone()));
                Fold::Ignored
            }
            EventKind::Usage { used, size } => {
                self.usage = Some((*used, *size));
                Fold::Ignored
            }
            EventKind::Options { options } => {
                self.options.clone_from(options);
                Fold::Ignored
            }
            EventKind::TurnStarted => {
                self.turn = Some((event.seq, event.at));
                self.plan = None;
                self.last_outcome = None;
                Fold::Ignored
            }
            EventKind::TurnEnded { outcome, .. } => {
                self.last_outcome = Some(*outcome);
                self.plan = None;
                let Some((seq, from)) = self.turn.take() else {
                    return Fold::Ignored;
                };
                self.turn_ended(seq, from, event, *outcome)
            }
            EventKind::Notice { text, .. } => self.push(Row {
                id: event.seq,
                kind: RowKind::Notice(text.clone()),
            }),
            _ => Fold::Ignored,
        }
    }

    fn push(&mut self, row: Row) -> Fold {
        self.rows.push(Rc::new(row));
        Fold::Dirty(self.rows.len() - 1)
    }

    pub(super) fn plan(&self) -> Option<(u64, &[PlanEntry])> {
        let (id, entries) = self.plan.as_ref()?;
        Some((*id, entries))
    }

    // The call a permission request waits on. It stays a work row, so the row says what waits.
    pub(super) fn approval(&self, id: &PendingInteractionId) -> Option<&ToolRow> {
        fn find<'a>(row: &'a Row, id: &PendingInteractionId) -> Option<&'a ToolRow> {
            match &row.kind {
                RowKind::Work(tool) if tool.call.approval.as_ref() == Some(id) => {
                    Some(tool.as_ref())
                }
                RowKind::Bundle(members) => members.iter().find_map(|member| find(member, id)),
                RowKind::Turn(turn) => turn.members.iter().find_map(|member| find(member, id)),
                _ => None,
            }
        }
        self.rows.iter().find_map(|row| find(row, id))
    }

    // A finished turn folds the rows before its answer into one row, so the answer stays in view.
    // A failed turn folds the same rows and stays open while it is the last row; a turn with no
    // work draws no row.
    fn turn_ended(
        &mut self,
        started: u64,
        from: Timestamp,
        event: &Event,
        outcome: TurnOutcome,
    ) -> Fold {
        // The turn's prompt comes before its start, so its rows are the ones after that start.
        let start = self
            .rows
            .iter()
            .position(|row| row.id > started)
            .unwrap_or(self.rows.len());
        let work = self.rows[start..]
            .iter()
            .rposition(|row| {
                matches!(
                    &row.kind,
                    RowKind::Thought(_) | RowKind::Work(_) | RowKind::Bundle(_)
                )
            })
            .map(|index| start + index);
        let Some(work) = work else {
            return Fold::Ignored;
        };
        let end = if outcome == TurnOutcome::Failed {
            self.rows.len()
        } else {
            let answer = self.rows.iter().rposition(|row| {
                matches!(
                    &row.kind,
                    RowKind::Message {
                        role: Role::Agent,
                        ..
                    }
                )
            });
            match answer {
                Some(answer) if answer > work => answer,
                _ => return Fold::Ignored,
            }
        };
        if end <= start {
            return Fold::Ignored;
        }
        let members: Vec<Row> = self
            .rows
            .drain(start..end)
            .map(|row| Rc::try_unwrap(row).unwrap_or_else(|row| (*row).clone()))
            .collect();
        self.rows.insert(
            start,
            Rc::new(Row {
                id: event.seq,
                kind: RowKind::Turn(Box::new(TurnRow {
                    from,
                    to: event.at,
                    outcome,
                    members,
                })),
            }),
        );
        Fold::Dirty(start)
    }

    fn find_tool(&self, id: &str) -> Option<(usize, Option<usize>)> {
        self.rows
            .iter()
            .enumerate()
            .rev()
            .find_map(|(top, row)| match &row.kind {
                RowKind::Work(tool) if tool.call.id == id => Some((top, None)),
                RowKind::Bundle(members) => members
                    .iter()
                    .position(
                        |member| matches!(&member.kind, RowKind::Work(tool) if tool.call.id == id),
                    )
                    .map(|member| (top, Some(member))),
                RowKind::Turn(turn) => turn
                    .members
                    .iter()
                    .position(
                        |member| matches!(&member.kind, RowKind::Work(tool) if tool.call.id == id),
                    )
                    .map(|member| (top, Some(member))),
                _ => None,
            })
    }

    fn chunk(
        &mut self,
        role: Role,
        text: &str,
        message_id: Option<String>,
        event: &Event,
        thought: bool,
    ) -> Fold {
        let last = self.rows.len().checked_sub(1);
        if let Some(index) = last {
            let row = Rc::make_mut(&mut self.rows[index]);
            let merges = match (&mut row.kind, thought) {
                (
                    RowKind::Message {
                        role: seen,
                        text: _,
                        message_id: seen_id,
                        ..
                    },
                    false,
                ) => *seen == role && *seen_id == message_id,
                (RowKind::Thought(seen), true) => seen.message_id == message_id,
                _ => false,
            };
            if merges {
                match &mut row.kind {
                    RowKind::Message { text: joined, .. } => joined.push_str(text),
                    RowKind::Thought(seen) => {
                        seen.text.push_str(text);
                        seen.to = event.at;
                    }
                    _ => {}
                }
                return Fold::Dirty(index);
            }
        }
        let kind = if thought {
            RowKind::Thought(Box::new(ThoughtRow {
                text: text.to_owned(),
                message_id,
                from: event.at,
                to: event.at,
            }))
        } else {
            RowKind::Message {
                role,
                text: text.to_owned(),
                message_id,
                at: event.at,
            }
        };
        self.push(Row {
            id: event.seq,
            kind,
        })
    }

    fn tool(&mut self, seq: u64, delta: ToolRow) -> Fold {
        if let Some((top, member)) = self.find_tool(&delta.call.id) {
            let row = Rc::make_mut(&mut self.rows[top]);
            let seen = match (&mut row.kind, member) {
                (RowKind::Work(tool), None) => Some(tool.as_mut()),
                (RowKind::Bundle(members), Some(member)) => match &mut members[member].kind {
                    RowKind::Work(tool) => Some(tool.as_mut()),
                    _ => None,
                },
                (RowKind::Turn(turn), Some(member)) => match &mut turn.members[member].kind {
                    RowKind::Work(tool) => Some(tool.as_mut()),
                    _ => None,
                },
                _ => None,
            };
            if let Some(seen) = seen {
                merge(seen, delta);
                return Fold::Dirty(self.bundle(top));
            }
        }
        self.rows.push(Rc::new(Row {
            id: seq,
            kind: RowKind::Work(Box::new(delta)),
        }));
        let index = self.rows.len() - 1;
        Fold::Dirty(self.bundle(index))
    }

    fn bundle(&mut self, index: usize) -> usize {
        let index = self.join_before(index);
        self.join_after(index)
    }

    fn join_before(&mut self, index: usize) -> usize {
        if index == 0 || !self.bundles(index) || !self.bundles(index - 1) {
            return index;
        }
        self.join(index - 1)
    }

    fn join_after(&mut self, index: usize) -> usize {
        if !self.bundles(index) || !self.bundles(index + 1) {
            return index;
        }
        self.join(index)
    }

    fn bundles(&self, index: usize) -> bool {
        match self.rows.get(index).map(|row| &row.kind) {
            Some(RowKind::Work(tool)) => tool.folds(),
            Some(RowKind::Bundle(_)) => true,
            _ => false,
        }
    }

    fn join(&mut self, first: usize) -> usize {
        let second = self.rows.remove(first + 1);
        let second = Rc::try_unwrap(second).unwrap_or_else(|row| (*row).clone());
        let lead = self.rows.remove(first);
        let lead = Rc::try_unwrap(lead).unwrap_or_else(|row| (*row).clone());
        let mut members = match lead.kind {
            RowKind::Bundle(members) => members,
            _ => vec![lead],
        };
        match second.kind {
            RowKind::Bundle(more) => members.extend(more),
            _ => members.push(second),
        }
        let id = members[0].id;
        self.rows.insert(
            first,
            Rc::new(Row {
                id,
                kind: RowKind::Bundle(members),
            }),
        );
        first
    }

    pub(super) fn live_last(&self) -> bool {
        match self.rows.last().map(|row| &row.kind) {
            Some(RowKind::Thought(thought)) => !thought.text.is_empty(),
            Some(RowKind::Work(tool)) => tool.live_body(),
            _ => false,
        }
    }

    pub(super) fn auto_open(&self, index: usize, answering: bool) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        let last = self.frontier() == Some(index);
        let failed = self.last_outcome == Some(TurnOutcome::Failed);
        match &row.kind {
            RowKind::Thought(thought) => last && answering && !thought.text.is_empty(),
            RowKind::Bundle(_) => last && answering,
            RowKind::Work(tool) => {
                last && ((answering && tool.live_body()) || (failed && tool.failed()))
            }
            RowKind::Turn(turn) => last && turn.outcome == TurnOutcome::Failed,
            _ => false,
        }
    }

    fn frontier(&self) -> Option<usize> {
        self.rows
            .iter()
            .rposition(|row| !matches!(row.kind, RowKind::Notice(_)))
    }

    pub(super) fn is_open(&self, index: usize, answering: bool) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        self.open
            .get(&row.key())
            .copied()
            .unwrap_or_else(|| self.auto_open(index, answering))
    }

    pub(super) fn member_choice(&self, key: RowKey) -> Option<bool> {
        self.open.get(&key).copied()
    }

    pub(super) fn toggle(&mut self, key: RowKey, answering: bool) -> Option<usize> {
        let top = self.place(key)?;
        let open = if self.rows.get(top)?.key() == key {
            self.is_open(top, answering)
        } else {
            self.open.get(&key).copied().unwrap_or(false)
        };
        self.open.insert(key, !open);
        Some(top)
    }

    // The top row that holds `key`: the row itself, or the fold it sits in.
    fn place(&self, key: RowKey) -> Option<usize> {
        fn holds(rows: &[Row], key: RowKey) -> bool {
            rows.iter().any(|row| {
                row.key() == key
                    || match &row.kind {
                        RowKind::Bundle(members) => holds(members, key),
                        RowKind::Turn(turn) => holds(&turn.members, key),
                        _ => false,
                    }
            })
        }
        self.rows.iter().enumerate().find_map(|(top, row)| {
            if row.key() == key {
                return Some(top);
            }
            match &row.kind {
                RowKind::Bundle(members) => holds(members, key).then_some(top),
                RowKind::Turn(turn) => holds(&turn.members, key).then_some(top),
                _ => None,
            }
        })
    }
}

fn diff_counts(diffs: &[ToolDiff]) -> (usize, usize) {
    diffs.iter().fold((0, 0), |(more, less), diff| {
        let old = diff.old.as_deref().unwrap_or_default();
        similar::TextDiff::from_lines(old, &diff.new)
            .iter_all_changes()
            .fold((more, less), |(more, less), change| match change.tag() {
                similar::ChangeTag::Insert => (more + 1, less),
                similar::ChangeTag::Delete => (more, less + 1),
                similar::ChangeTag::Equal => (more, less),
            })
    })
}

fn merge(seen: &mut ToolRow, delta: ToolRow) {
    if !delta.call.diff.is_empty() {
        seen.counts = diff_counts(&delta.call.diff);
    }
    seen.call.merge(delta.call);
    seen.to = delta.to;
}

#[cfg(test)]
mod tests;
