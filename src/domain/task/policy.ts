import type { ColumnType, HelperRole } from "./types"
import { parseOptions } from "../options"
import { columnMoveDenyReason as moveDenyReason, type ColumnMoveContext } from "./moves"
import {
  intakeReady as intakeIsReady,
  pendingRequiredIntakeDecisions as pendingIntakeDecisions,
  refinedPrompt,
  type IntakeDecision,
} from "./intake"
import { isResolvedFinding } from "./findings"
import { kagan } from "./metadata"

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
function canRetryHelper(metadata: Record<string, unknown> | undefined, role: HelperRole) {
  const { outcome, sessionID } = helper(metadata, role)
  return (
    outcome !== "ran" &&
    (kagan(metadata).helperError?.role === role || outcome !== undefined || sessionID !== undefined)
  )
}
export function canRetrySession(status: ColumnType, metadata: Record<string, unknown> | undefined) {
  return (
    (status === "backlog" && canRetryHelper(metadata, "intake")) ||
    (status === "review" && canRetryHelper(metadata, "validator"))
  )
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
