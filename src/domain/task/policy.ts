import { DEFAULT_IN_PROGRESS_CAP, type ColumnType, type HelperRole } from "./types"
import { parseOptions } from "../options"
import {
  intakeReady as intakeIsReady,
  pendingRequiredIntakeDecisions as pendingIntakeDecisions,
  refinedPrompt,
  type IntakeDecision,
} from "./intake"
import { isResolvedFinding } from "./findings"
import { kagan } from "./metadata"

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

function moveDenyReason(
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

export function inProgressCap(options?: Record<string, unknown>) {
  return parseOptions(options).inProgressLimit
}
export function helperRetries(options?: Record<string, unknown>) {
  return parseOptions(options).helperRetries
}
export function sendBackStopThreshold(options?: Record<string, unknown>) {
  return parseOptions(options).sendBackStopThreshold
}
export function squashMerge(options?: Record<string, unknown>) {
  return parseOptions(options).squashMerge
}
export function helper(metadata: Record<string, unknown> | undefined, role: HelperRole) {
  const view = kagan(metadata)
  const raw =
    role === "intake"
      ? {
          sessionID: view.intakeSessionID,
          outcome: view.intakeOutcome,
          attempts: view.intakeAttempts,
          parent: view.intakeParent,
        }
      : {
          sessionID: view.validatorSessionID,
          outcome: view.validatorOutcome,
          attempts: view.validatorAttempts,
          parent: view.validatorParent,
        }
  return { ...raw, attempts: raw.attempts !== undefined && raw.attempts >= 0 ? raw.attempts : 0 }
}
export function intakeReady(metadata?: Record<string, unknown>) {
  return intakeIsReady(helper(metadata, "intake").outcome, kagan(metadata).intake)
}
export function pendingRequiredIntakeDecisions(metadata?: Record<string, unknown>): IntakeDecision[] {
  return pendingIntakeDecisions(kagan(metadata).intake)
}
export function getRefinedPrompt(metadata?: Record<string, unknown>) {
  return refinedPrompt(kagan(metadata).intake)
}
export function pendingFindingCount(metadata?: Record<string, unknown>) {
  return (kagan(metadata).findings ?? []).filter((finding) => !isResolvedFinding(finding)).length
}
export function isReadOnlyHelperRole(role: string | undefined): role is HelperRole {
  return role === "intake" || role === "validator"
}
export function isSupervisedSession(metadata?: Record<string, unknown>) {
  const view = kagan(metadata)
  return (
    view.boardTask === true ||
    view.role !== undefined ||
    helper(metadata, "intake").parent !== undefined ||
    helper(metadata, "validator").parent !== undefined ||
    view.workerParent !== undefined
  )
}
function helperEverSpawned(metadata: Record<string, unknown> | undefined, role: HelperRole) {
  const view = kagan(metadata)
  const { outcome, sessionID, attempts } = helper(metadata, role)
  if (sessionID !== undefined || outcome !== undefined || attempts > 0) return true
  if (role === "intake" && view.intake !== undefined) return true
  if (role === "validator" && (view.findings !== undefined || view.check !== undefined)) return true
  return view.helperError?.role === role
}
export function canRestartHelper(status: ColumnType, metadata: Record<string, unknown> | undefined) {
  return (
    (status === "backlog" && helperEverSpawned(metadata, "intake")) ||
    (status === "review" && helperEverSpawned(metadata, "validator"))
  )
}
/** @deprecated Use canRestartHelper */
export function canRetrySession(status: ColumnType, metadata: Record<string, unknown> | undefined) {
  return canRestartHelper(status, metadata)
}
export function needsHuman(status: ColumnType, metadata: Record<string, unknown> | undefined) {
  return kagan(metadata).awaitingInput !== undefined || (status === "review" && kagan(metadata).approved !== true)
}
export function countInProgressForMove(
  sessions: readonly { id: string; parentID?: string | null; status: ColumnType }[],
  sessionID: string,
  source: ColumnType,
) {
  return sessions.filter(
    (session) =>
      !session.parentID && session.status === "in_progress" && (session.id !== sessionID || source === "in_progress"),
  ).length
}
export function columnMoveDenyReason(to: ColumnType, metadata?: Record<string, unknown>, ctx?: ColumnMoveContext) {
  const view = kagan(metadata)
  return moveDenyReason(
    to,
    view,
    { ready: intakeReady(metadata), pendingDecisions: pendingRequiredIntakeDecisions(metadata).length },
    ctx,
  )
}
export function approveDenyReason(metadata?: Record<string, unknown>) {
  const view = kagan(metadata)
  if (view.boardTask !== true) return "Only board tasks can be approved"
  const outcome = helper(metadata, "validator").outcome
  if (outcome !== "ran" && outcome !== "failed") return "Review hasn't finished — no validator outcome yet"
  const pending = pendingFindingCount(metadata)
  return pending > 0 ? `${pending} finding(s) need triage` : undefined
}
export function helperRestartPatch(role: HelperRole): Record<string, unknown> {
  const patch: Record<string, unknown> = {
    [`${role}SessionID`]: undefined,
    [`${role}Outcome`]: undefined,
    [`${role}Attempts`]: 0,
    helperError: undefined,
  }
  if (role === "intake") patch.intake = undefined
  else {
    patch.findings = undefined
    patch.check = undefined
    patch.approved = undefined
  }
  return patch
}
export function nextGenerationPatch(metadata?: Record<string, unknown>): Record<string, unknown> {
  const view = kagan(metadata)
  const carried = (view.findings ?? []).filter(
    (finding) => finding.resolution === "intended" || finding.resolution === "ignored",
  )
  const priorTriage = [...(view.priorTriage ?? []), ...carried]
  return {
    generation: view.generation + 1,
    priorTriage: priorTriage.length > 0 ? priorTriage : undefined,
    findings: undefined,
    check: undefined,
    validatorSessionID: undefined,
    validatorOutcome: undefined,
    validatorAttempts: undefined,
    helperError: undefined,
    approved: undefined,
  }
}
