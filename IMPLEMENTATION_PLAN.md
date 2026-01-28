# Kagan Rewrite Plan (Alpha, Breaking Changes)

## Context & Decision Summary
This plan reflects a radical rewrite for an early‑alpha product where breaking changes are expected.
Key decisions that shape the tasks:
- Remove the reviewer agent concept; all agents are Workers that change behavior via hats.
- Tickets have types: AUTO (default) and PAIR; user can switch at any time.
- Review column is a single column split into two lanes: AI Review (top) and Human Review (bottom).
- Auto tickets go through AI Review automatically; Pair tickets skip AI Review by default but allow opt‑in AI review.
- Human Review is always required, regardless of ticket type.
- Pair programming uses a dedicated screen with chat + delegation list + tool launchers.
- Delegated tasks must be first‑class records (not just text in ticket description).
- The TUI must stay tool‑agnostic: per‑ticket overrides for agent backend, IDE, and diff tool.
- Planner mode becomes dialog‑like, gathers requirements, requires explicit approval, then splits into multiple tickets and asks for AUTO vs PAIR assignment.

## Constraints & Quality Gates
- No migrations or backwards compatibility. Fresh schema expected.
- Code style: ultra‑laconic, modern, idiomatic Python; Textual best practices.
- File size guideline: keep modules ~150–250 LOC; split if needed.
- Quality gates per batch:
  - `uv run poe fix`
  - `uv run poe typecheck`
  - `uv run pytest tests/ -v`

## Parallelizable Batches (Non‑overlapping)
Each batch is independent and can be assigned to separate subagents.

### Batch A — Data Model + Schema (DB‑only)
Goal: new schema for tickets, delegations, and (optional) pair messages.
- Update `src/kagan/database/schema.sql` (no migrations, replace/extend schema):
  - Add `ticket_type` (AUTO/PAIR), `review_state` (AI_REVIEW/HUMAN_REVIEW),
    `handoff_summary`, `ide_command`, `diff_command`.
  - Add `delegations` table (ticket‑scoped tasks).
  - Optional: add `pair_messages` table (if we choose to persist chat).
- Update `src/kagan/database/models.py`:
  - Add `TicketType`, `ReviewState` enums.
  - Extend `Ticket`, `TicketCreate`, `TicketUpdate` with new fields.
  - Add `Delegation` models (create/update).
- Update `src/kagan/database/manager.py`:
  - CRUD for delegations (+ messages if included).
  - Ticket CRUD to include new fields.
- Update `tests/test_models.py` and `tests/test_database.py` for new fields.

Deliverable: schema + models + DB manager compile and tests updated.

### Batch B — Agent System + Scheduler Logic (No UI)
Goal: remove reviewer agent concept and implement self‑review via hats + new review_state.
- Remove reviewer role usage:
  - Update `src/kagan/agents/roles.py` (remove reviewer role, keep worker/planner).
  - Delete/retire `src/kagan/agents/reviewer.py` or refactor into shared helper functions.
- Scheduler changes in `src/kagan/agents/scheduler.py`:
  - Respect `ticket_type` (AUTO handled by scheduler, PAIR ignored unless explicit).
  - Replace REVIEW loop with self‑review flow using worker + reviewer hat.
  - On auto completion: set `status=REVIEW`, `review_state=AI_REVIEW`.
  - Self‑review: run reviewer prompt using same worker agent (reviewer hat).
  - If approved: set `review_state=HUMAN_REVIEW` (do NOT auto‑DONE).
  - If rejected: return to `IN_PROGRESS` with note in scratchpad.
- Update prompts if needed:
  - Adjust `src/kagan/prompts/review.md` to align with self‑review output.
  - Keep `PromptLoader` paths intact.
- Update tests: `tests/test_scheduler.py`, `tests/test_signals.py`.

Deliverable: scheduler flow updated, reviewer agent removed, tests pass.

### Batch C — Kanban UI + Review Split + Ticket Details (UI‑only)
Goal: show type/review state, split Review column, add pair entry.
- `src/kagan/constants.py`:
  - Add labels for review state and ticket type.
  - Update column order only if required (still 4 columns).
- `src/kagan/ui/widgets/column.py`:
  - Special handling for REVIEW column to render two sections (AI/Human).
- `src/kagan/ui/widgets/card.py`:
  - Show type badge + review state badge when relevant.
- `src/kagan/ui/screens/kanban.py`:
  - Add binding `p` to open Pair screen.
  - Update move/drag logic to set review_state defaults:
    - Auto → AI_REVIEW; Pair → HUMAN_REVIEW.
  - Prevent Done unless review_state=HUMAN_REVIEW.
- `src/kagan/ui/modals/ticket_details.py`:
  - Add ticket type selector.
  - Add per‑ticket tool overrides (IDE/diff) + agent backend override.
- `src/kagan/styles/kagan.tcss`:
  - Add styles for badges and split Review layout.
- Update snapshot/UI tests: `tests/test_ui.py`, `tests/test_snapshots.py`.

Deliverable: board reflects new states; ticket details edit supports new fields.

### Batch D — Pair Programming Screen + Delegation UX
Goal: new `PairScreen` with chat + delegations + tool launching.
- Add `src/kagan/ui/screens/pair.py` (new screen):
  - Lean split layout: chat stream + delegation list + summary panel.
  - Actions: delegate task, open IDE, open diff tool, pause/resume agent,
    request AI review, switch to auto, back.
- Wire navigation:
  - From Kanban (`p`) open Pair screen for selected ticket.
  - From Ticket details, optional “Open Pair” action.
- DB wiring:
  - Delegation CRUD operations (new manager methods from Batch A).
  - Delegation list in UI, update statuses.
- Handoff summary behavior:
  - Auto→Pair: pause agent, write summary to ticket.
  - Pair→Auto: write summary, resume worker with summary context.
- Add tests for screen navigation + delegation CRUD.

Deliverable: Pair screen functional and integrated with delegations.

### Batch E — Planner Dialog Rewrite (Requirements → Approval → Ticket Split)
Goal: dialog‑like planner that creates multiple tickets and type selection.
- Replace planner logic in `src/kagan/agents/planner.py`:
  - New output format for multiple tickets and ticket types.
- Update `src/kagan/ui/screens/chat.py`:
  - Dialog‑style interaction with explicit approval step.
  - After approval, parse and create multiple tickets with types set.
  - Ask user to classify auto vs pair before creation (if missing in output).
- Tests: `tests/test_chat_screen.py`, `tests/test_prompt_loader.py`.

Deliverable: planner produces multiple tickets with correct type + acceptance criteria.

### Batch F — Tool‑Agnostic Integrations
Goal: per‑ticket tool overrides and config defaults.
- Config changes in `src/kagan/config.py`:
  - Add defaults for `ide_command` and `diff_command` (OS specific).
- Use overrides in Pair screen:
  - Prefer ticket overrides; fallback to config defaults.
- Update welcome screen if needed to collect defaults (optional).
- Tests for command resolution logic.

Deliverable: IDE/diff launch is configurable per ticket with defaults.

## Execution Order
- First: Batch A (schema + models). Required by all others.
- Then in parallel: B, C, F.
- After B + A: D (Pair screen) can proceed.
- After A + E: Planner rewrite.

## Decision Points (Already Set)
- Pair tickets skip AI Review by default; optional “Request AI Review.”
- No migrations or backwards compatibility.
- Human Review always required.
