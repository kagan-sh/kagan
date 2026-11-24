use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use kg_domain::{
    Event, EventKind, PendingInteractionId, PlanEntry, Role, Timestamp, ToolDiff, ToolKind,
    ToolLocation, ToolStatus, TurnOutcome, WorkKind,
};

use super::kind;

/// The parts a transcript can hold, each drawn by its own function and named in its element id, so
/// a plugin can replace one later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Part {
    Message,
    Thought,
    Work,
    Bundle,
    Plan,
    Approval,
    Notice,
}

impl Part {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Message => "message",
            Self::Thought => "thought",
            Self::Work => "work",
            Self::Bundle => "bundle",
            Self::Plan => "plan",
            Self::Approval => "approval",
            Self::Notice => "notice",
        }
    }
}

/// One row of the transcript. `id` is the `seq` of the first event the row came from, so it is
/// stable across appends and survives a page of older events.
#[derive(Clone, Debug)]
pub(super) struct Row {
    pub(super) id: u64,
    pub(super) kind: RowKind,
}

/// What the person's choice is kept under: a bundle's id is its first member's, so the two rows
/// need a key apart.
pub(super) type RowKey = (bool, u64);

impl Row {
    pub(super) fn key(&self) -> RowKey {
        (matches!(self.kind, RowKind::Bundle(_)), self.id)
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
    Plan(Vec<PlanEntry>),
    Approval(PendingInteractionId),
    Notice(String),
    Divider {
        from: Timestamp,
        to: Timestamp,
        outcome: TurnOutcome,
    },
}

#[derive(Clone, Debug)]
pub(super) struct ThoughtRow {
    pub(super) text: String,
    pub(super) message_id: Option<String>,
    pub(super) from: Timestamp,
    pub(super) to: Timestamp,
}

/// The fold of every event with one tool id.
#[derive(Clone, Debug)]
pub(super) struct ToolRow {
    pub(super) id: String,
    pub(super) name: Option<String>,
    pub(super) kind: Option<ToolKind>,
    pub(super) title: Option<String>,
    pub(super) status: Option<ToolStatus>,
    pub(super) input: Option<serde_json::Value>,
    pub(super) output: Option<String>,
    pub(super) diff: Vec<ToolDiff>,
    /// The lines the diff adds and removes, counted once as the diff arrives, never while drawing.
    pub(super) counts: (usize, usize),
    pub(super) locations: Vec<ToolLocation>,
    pub(super) approval: Option<PendingInteractionId>,
    pub(super) from: Timestamp,
    pub(super) to: Timestamp,
}

/// How far a tool call has got. A call the agent named but has not started is waiting, not running.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ToolStage {
    Waiting,
    Running,
    Settled,
}

impl ToolRow {
    /// The call's stage, which chooses its word, its mark and whether its body is live.
    pub(super) fn stage(&self) -> ToolStage {
        match self.status {
            Some(ToolStatus::Pending) => ToolStage::Waiting,
            Some(ToolStatus::InProgress) => ToolStage::Running,
            _ => ToolStage::Settled,
        }
    }

    /// A row that has not settled: its label shimmers and its meta is the live span.
    pub(super) fn running(&self) -> bool {
        self.stage() != ToolStage::Settled
    }

    pub(super) fn failed(&self) -> bool {
        self.status == Some(ToolStatus::Failed)
    }

    /// Whether the fold carries something a person can see inside the row.
    pub(super) fn live_body(&self) -> bool {
        self.running() && self.output.is_some() && self.kind != Some(ToolKind::Execute)
    }

    /// A finished look at the codebase, which folds into a bundle. An MCP call or a skill load
    /// the agent reports as a read is still work, so what the row is decides, once.
    pub(super) fn folds(&self) -> bool {
        self.status == Some(ToolStatus::Completed)
            && matches!(
                kind::of(self),
                WorkKind::Read | WorkKind::Search | WorkKind::Fetch
            )
    }

    pub(super) fn span(&self) -> Option<Duration> {
        let span = super::kind::span_of(self.from, self.to);
        (!span.is_zero()).then_some(span)
    }
}

/// What changed when an event was folded, so the view can keep the list's item count and measured
/// heights in step without rebuilding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fold {
    Ignored,
    /// Every row from here on is new or changed, so the list measures from it to the end.
    Dirty(usize),
}

/// The transcript as rows: the fold of the thread's events. One function applies one event, so the
/// view never rebuilds the whole list.
#[derive(Default)]
pub(super) struct Transcript {
    /// Rows are shared with the renderer, which holds one for the frame, so a frame never copies
    /// a body's output or a diff to draw it.
    pub(super) rows: Vec<Rc<Row>>,
    pub(super) usage: Option<(u64, u64)>,
    pub(super) last_outcome: Option<TurnOutcome>,
    /// The person's own expand and collapse choices: they always win over the rules.
    open: BTreeMap<RowKey, bool>,
    last_seq: Option<u64>,
    turn: Option<Timestamp>,
    plan: Option<usize>,
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
            EventKind::Tool {
                id,
                name,
                kind,
                title,
                status,
                input,
                output,
                diff,
                locations,
                approval,
            } => {
                let delta = ToolRow {
                    id: id.clone(),
                    name: name.clone(),
                    kind: *kind,
                    title: title.clone(),
                    status: *status,
                    input: input.clone(),
                    output: output.clone(),
                    diff: diff.clone(),
                    counts: diff_counts(diff),
                    locations: locations.clone(),
                    approval: approval.clone(),
                    from: event.at,
                    to: event.at,
                };
                let fold = self.tool(event.seq, delta);
                if let Some(approval) = approval {
                    self.push(Row {
                        id: event.seq,
                        kind: RowKind::Approval(approval.clone()),
                    });
                }
                fold
            }
            EventKind::Plan { entries } => {
                if let Some(index) = self.plan.filter(|index| *index < self.rows.len()) {
                    Rc::make_mut(&mut self.rows[index]).kind = RowKind::Plan(entries.clone());
                    Fold::Dirty(index)
                } else {
                    self.plan = Some(self.rows.len());
                    self.push(Row {
                        id: event.seq,
                        kind: RowKind::Plan(entries.clone()),
                    })
                }
            }
            EventKind::Usage { used, size } => {
                self.usage = Some((*used, *size));
                Fold::Ignored
            }
            EventKind::TurnStarted => {
                self.turn = Some(event.at);
                self.plan = None;
                self.last_outcome = None;
                Fold::Ignored
            }
            EventKind::TurnEnded { outcome, .. } => {
                self.last_outcome = Some(*outcome);
                self.plan = None;
                let Some(from) = self.turn.take() else {
                    return Fold::Ignored;
                };
                self.push(Row {
                    id: event.seq,
                    kind: RowKind::Divider {
                        from,
                        to: event.at,
                        outcome: *outcome,
                    },
                })
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

    /// The agent's plan as it stands, with the id of the row it draws as, so the card above the
    /// prompt box can show it without a second copy.
    pub(super) fn plan(&self) -> Option<(u64, &[PlanEntry])> {
        let row = self.rows.get(self.plan?)?;
        match &row.kind {
            RowKind::Plan(entries) => Some((row.id, entries)),
            _ => None,
        }
    }

    /// The row a tool id's events fold into: its place in the transcript, and its place among a
    /// bundle's members when it has one.
    fn find_tool(&self, id: &str) -> Option<(usize, Option<usize>)> {
        self.rows
            .iter()
            .enumerate()
            .rev()
            .find_map(|(top, row)| match &row.kind {
                RowKind::Work(tool) if tool.id == id => Some((top, None)),
                RowKind::Bundle(members) => members
                    .iter()
                    .position(|member| matches!(&member.kind, RowKind::Work(tool) if tool.id == id))
                    .map(|member| (top, Some(member))),
                _ => None,
            })
    }

    /// Appends a message or a thought chunk to the row it belongs to, or starts a row.
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

    /// Folds one tool delta into the row with its id, wherever that row is, or starts a row.
    fn tool(&mut self, seq: u64, delta: ToolRow) -> Fold {
        if let Some((top, member)) = self.find_tool(&delta.id) {
            let row = Rc::make_mut(&mut self.rows[top]);
            let seen = match (&mut row.kind, member) {
                (RowKind::Work(tool), None) => Some(tool.as_mut()),
                (RowKind::Bundle(members), Some(member)) => match &mut members[member].kind {
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

    /// Folds the row at `index` with a finished neighbour on either side, so a row that finishes
    /// last still finds its pair, and names the row the list has to measure: the bundle's place, or
    /// the row's own.
    fn bundle(&mut self, index: usize) -> usize {
        let index = self.join_before(index);
        self.join_after(index)
    }

    /// Folds the row at `index` with the row before it when the two are one bundle.
    fn join_before(&mut self, index: usize) -> usize {
        if index == 0 || !self.bundles(index) || !self.bundles(index - 1) {
            return index;
        }
        self.join(index - 1)
    }

    /// Folds the row after `index` into it when the two are one bundle: two reads that finish out
    /// of order join at the second one's finish, not before it.
    fn join_after(&mut self, index: usize) -> usize {
        if !self.bundles(index) || !self.bundles(index + 1) {
            return index;
        }
        self.join(index)
    }

    /// Whether the row at `index` is a bundle, or finished looking work that folds into one.
    fn bundles(&self, index: usize) -> bool {
        match self.rows.get(index).map(|row| &row.kind) {
            Some(RowKind::Work(tool)) => tool.folds(),
            Some(RowKind::Bundle(_)) => true,
            _ => false,
        }
    }

    /// Puts the rows at `first` and `first + 1` into one bundle at `first`.
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

    /// The last row has something streaming into it, so it is the live frontier and no "Working…"
    /// row is needed under it.
    pub(super) fn live_last(&self) -> bool {
        match self.rows.last().map(|row| &row.kind) {
            Some(RowKind::Thought(thought)) => !thought.text.is_empty(),
            Some(RowKind::Work(tool)) => tool.live_body(),
            _ => false,
        }
    }

    /// Whether the rules open the row: the live frontier of a running turn, and a failure that
    /// ended the turn.
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
            _ => false,
        }
    }

    /// The last row a reader reads as content: the divider under a turn is a quiet line, and so is
    /// a notice, so a failure that ended the turn stays the last row it reads.
    fn frontier(&self) -> Option<usize> {
        self.rows
            .iter()
            .rposition(|row| !matches!(row.kind, RowKind::Divider { .. } | RowKind::Notice(_)))
    }

    /// Whether the row is open: the person's choice, else the rules.
    pub(super) fn is_open(&self, index: usize, answering: bool) -> bool {
        let Some(row) = self.rows.get(index) else {
            return false;
        };
        self.open
            .get(&row.key())
            .copied()
            .unwrap_or_else(|| self.auto_open(index, answering))
    }

    /// Whether a row inside a bundle is open: only a person opens one.
    pub(super) fn member_open(&self, key: RowKey) -> bool {
        self.open.get(&key).copied().unwrap_or(false)
    }

    /// Flips the row's own choice and names the row the list must measure, or nothing when no row
    /// has that key.
    pub(super) fn toggle(&mut self, key: RowKey, answering: bool) -> Option<usize> {
        let (top, member) = self.place(key)?;
        let open = match member {
            Some(_) => self.open.get(&key).copied().unwrap_or(false),
            None => self.is_open(top, answering),
        };
        self.open.insert(key, !open);
        Some(top)
    }

    /// Where the row with this key is: its place in the transcript, and its place among a bundle's
    /// members when it is one.
    fn place(&self, key: RowKey) -> Option<(usize, Option<usize>)> {
        self.rows
            .iter()
            .enumerate()
            .find_map(|(top, row)| match &row.kind {
                RowKind::Bundle(members) => {
                    if row.key() == key {
                        return Some((top, None));
                    }
                    members
                        .iter()
                        .position(|member| member.key() == key)
                        .map(|member| (top, Some(member)))
                }
                _ => (row.key() == key).then_some((top, None)),
            })
    }
}

/// The lines a diff adds and removes, counted once as the diff arrives.
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

/// Keeps every field the update names and leaves the rest as they were.
fn merge(seen: &mut ToolRow, delta: ToolRow) {
    if delta.name.is_some() {
        seen.name = delta.name;
    }
    if delta.kind.is_some() {
        seen.kind = delta.kind;
    }
    if delta.title.is_some() {
        seen.title = delta.title;
    }
    if delta.status.is_some() {
        seen.status = delta.status;
    }
    if delta.input.is_some() {
        seen.input = delta.input;
    }
    if delta.output.is_some() {
        seen.output = delta.output;
    }
    if !delta.diff.is_empty() {
        seen.counts = diff_counts(&delta.diff);
        seen.diff = delta.diff;
    }
    if !delta.locations.is_empty() {
        seen.locations = delta.locations;
    }
    if delta.approval.is_some() {
        seen.approval = delta.approval;
    }
    seen.to = delta.to;
}

#[cfg(test)]
mod tests;
