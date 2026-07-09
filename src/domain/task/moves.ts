import { DEFAULT_IN_PROGRESS_CAP, type ColumnType } from "./types"

export type ColumnMoveContext = {
  inProgressCount: number
  source?: ColumnType
  cap?: number
}

type MoveTask = { worktree?: string; approved?: boolean; startedAt?: number }

type IntakeStatus = { ready: boolean; pendingDecisions: number }

function wipLimitDenyReason(to: ColumnType, ctx: ColumnMoveContext | undefined): string | undefined {
  const cap = ctx?.cap ?? DEFAULT_IN_PROGRESS_CAP
  const inProgressCount = ctx?.inProgressCount ?? 0
  if (to !== "in_progress" || ctx?.source === "in_progress" || inProgressCount < cap) return undefined
  return `In Progress WIP limit of ${cap} reached`
}

function backlogStartDenyReason(
  to: ColumnType,
  task: MoveTask,
  intake: IntakeStatus,
  source: ColumnType | undefined,
): string | undefined {
  if (to !== "in_progress" || source !== "backlog") return undefined
  if (task.worktree === undefined)
    return "Task has no isolated worktree — create tasks from the board so agents run sandboxed"
  if (intake.ready) return undefined
  return intake.pendingDecisions > 0
    ? `${intake.pendingDecisions} intake decision(s) need your answer before starting`
    : "Intake is still being prepared"
}

function finalColumnDenyReason(to: ColumnType, task: MoveTask, source: ColumnType | undefined): string | undefined {
  if (to === "done" && task.approved !== true) return "Task must be approved before moving to Done"
  if (to === "backlog" && source === "in_progress" && task.startedAt !== undefined) {
    return "Agent already started — let it finish, send it back from Review, or delete the task"
  }
  if (to !== "done" && source === "done") return "Approved tasks stay in Done — create a follow-up task instead"
  return undefined
}

export function columnMoveDenyReason(
  to: ColumnType,
  task: MoveTask,
  intake: IntakeStatus,
  ctx?: ColumnMoveContext,
): string | undefined {
  return [
    wipLimitDenyReason(to, ctx),
    backlogStartDenyReason(to, task, intake, ctx?.source),
    finalColumnDenyReason(to, task, ctx?.source),
  ].find((reason): reason is string => reason !== undefined)
}
