use std::time::Duration;

use kg_domain::{
    Event, EventKind, PendingInteractionId, PlanEntry, Role, Timestamp, ToolDiff, ToolKind,
    ToolLocation, ToolStatus, TurnOutcome,
};

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
    pub(super) locations: Vec<ToolLocation>,
    pub(super) approval: Option<PendingInteractionId>,
    pub(super) from: Timestamp,
    pub(super) to: Timestamp,
}

impl ToolRow {
    /// A row that is still running: its label shimmers and its meta is the live span.
    pub(super) fn running(&self) -> bool {
        matches!(
            self.status,
            Some(ToolStatus::Pending | ToolStatus::InProgress)
        )
    }

    pub(super) fn failed(&self) -> bool {
        self.status == Some(ToolStatus::Failed)
    }

    /// Whether the fold carries something a person can see inside the row.
    pub(super) fn live_body(&self) -> bool {
        self.running() && self.output.is_some() && self.kind != Some(ToolKind::Execute)
    }

    /// A finished look at the codebase, which folds into a bundle.
    pub(super) fn folds(&self) -> bool {
        self.status == Some(ToolStatus::Completed)
            && matches!(
                self.kind,
                Some(ToolKind::Read | ToolKind::Search | ToolKind::Fetch)
            )
    }

    pub(super) fn span(&self) -> Option<Duration> {
        let span = self.to.0.saturating_sub(self.from.0);
        (span > 0).then(|| Duration::from_millis(u64::try_from(span).unwrap_or(0)))
    }
}

/// What changed when an event was folded, so the view can keep the list's item count and measured
/// heights in step without rebuilding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Fold {
    Ignored,
    Appended(usize),
    Changed(usize),
}

/// The transcript as rows: the fold of the thread's events. One function applies one event, so the
/// view never rebuilds the whole list.
#[derive(Default)]
pub(super) struct Transcript {
    pub(super) rows: Vec<Row>,
    pub(super) usage: Option<(u64, u64)>,
    pub(super) last_outcome: Option<TurnOutcome>,
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
                    self.rows[index].kind = RowKind::Plan(entries.clone());
                    Fold::Changed(index)
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
        self.rows.push(row);
        Fold::Appended(self.rows.len() - 1)
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
            let merges = match (&mut self.rows[index].kind, thought) {
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
                match &mut self.rows[index].kind {
                    RowKind::Message { text: joined, .. } => joined.push_str(text),
                    RowKind::Thought(seen) => {
                        seen.text.push_str(text);
                        seen.to = event.at;
                    }
                    _ => {}
                }
                return Fold::Changed(index);
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

    /// Folds one tool delta into the last row with its id, or starts a row. A finished look folds
    /// into the bundle at the tail when there is one.
    fn tool(&mut self, seq: u64, delta: ToolRow) -> Fold {
        let id = delta.id.clone();
        if let Some(index) = self.rows.len().checked_sub(1)
            && let RowKind::Work(seen) = &mut self.rows[index].kind
            && seen.id == id
        {
            merge(seen, delta);
            return Fold::Changed(index);
        }
        let row = Row {
            id: seq,
            kind: RowKind::Work(Box::new(delta)),
        };
        let folds = self.rows.last().is_some_and(|last| {
            matches!(&last.kind, RowKind::Work(tool) if tool.folds())
                || matches!(&last.kind, RowKind::Bundle(_))
        });
        let completed = matches!(&row.kind, RowKind::Work(tool) if tool.folds());
        if completed && folds {
            let index = self.rows.len() - 1;
            let last = self.rows.remove(index);
            let members = match last.kind {
                RowKind::Bundle(mut members) => {
                    members.push(row);
                    members
                }
                _ => vec![last, row],
            };
            let id = members[0].id;
            self.rows.push(Row {
                id,
                kind: RowKind::Bundle(members),
            });
            return Fold::Changed(index);
        }
        self.push(row)
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
mod tests {
    use super::*;
    use kg_domain::{EventKind, ThreadId};

    fn event(seq: u64, kind: EventKind) -> Event {
        Event {
            thread_id: ThreadId("thr".into()),
            seq,
            at: Timestamp(i64::try_from(seq).unwrap() * 1000),
            kind,
        }
    }

    fn tool(seq: u64, kind: ToolKind, status: ToolStatus) -> Event {
        event(
            seq,
            EventKind::Tool {
                id: format!("call-{seq}"),
                name: None,
                kind: Some(kind),
                title: Some("Read file".into()),
                status: Some(status),
                input: None,
                output: None,
                diff: Vec::new(),
                locations: Vec::new(),
                approval: None,
            },
        )
    }

    #[test]
    fn three_finished_reads_fold_into_one_bundle_and_an_edit_does_not() {
        let mut transcript = Transcript::default();
        for seq in 1..=3 {
            transcript.absorb(&tool(seq, ToolKind::Read, ToolStatus::Completed));
        }
        transcript.absorb(&tool(4, ToolKind::Edit, ToolStatus::Completed));
        assert_eq!(transcript.rows.len(), 2);
        let RowKind::Bundle(members) = &transcript.rows[0].kind else {
            panic!("a bundle");
        };
        assert_eq!(members.len(), 3);
        assert_eq!(transcript.rows[0].id, 1);
        assert!(matches!(transcript.rows[1].kind, RowKind::Work(_)));
    }

    #[test]
    fn a_single_finished_read_stays_a_row() {
        let mut transcript = Transcript::default();
        transcript.absorb(&tool(1, ToolKind::Read, ToolStatus::Completed));
        assert!(matches!(transcript.rows[0].kind, RowKind::Work(_)));
    }

    #[test]
    fn a_running_read_never_folds() {
        let mut transcript = Transcript::default();
        transcript.absorb(&tool(1, ToolKind::Read, ToolStatus::InProgress));
        transcript.absorb(&tool(2, ToolKind::Read, ToolStatus::Completed));
        assert_eq!(transcript.rows.len(), 2, "the running row is a row");
    }

    #[test]
    fn a_plan_replaces_the_turns_plan_row() {
        let mut transcript = Transcript::default();
        transcript.absorb(&event(1, EventKind::TurnStarted));
        transcript.absorb(&event(
            2,
            EventKind::Plan {
                entries: vec![PlanEntry {
                    text: "one".into(),
                    status: kg_domain::PlanStatus::Pending,
                }],
            },
        ));
        transcript.absorb(&event(
            3,
            EventKind::Plan {
                entries: vec![
                    PlanEntry {
                        text: "one".into(),
                        status: kg_domain::PlanStatus::Completed,
                    },
                    PlanEntry {
                        text: "two".into(),
                        status: kg_domain::PlanStatus::InProgress,
                    },
                ],
            },
        ));
        let plans: Vec<_> = transcript
            .rows
            .iter()
            .filter(|row| matches!(row.kind, RowKind::Plan(_)))
            .collect();
        assert_eq!(plans.len(), 1);
        assert_eq!(plans[0].id, 2);
        let RowKind::Plan(entries) = &plans[0].kind else {
            unreachable!()
        };
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn an_old_event_is_not_folded_twice() {
        let mut transcript = Transcript::default();
        assert_eq!(
            transcript.absorb(&event(2, EventKind::Started)),
            Fold::Ignored
        );
        assert_eq!(
            transcript.absorb(&event(1, EventKind::Started)),
            Fold::Ignored
        );
        assert_eq!(
            transcript.absorb(&event(
                3,
                EventKind::Notice {
                    severity: kg_domain::NoticeSeverity::Info,
                    text: "hi".into(),
                }
            )),
            Fold::Appended(0)
        );
        assert_eq!(
            transcript.absorb(&event(2, EventKind::Started)),
            Fold::Ignored
        );
    }
}
