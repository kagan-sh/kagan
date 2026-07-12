import { kagan } from "../../../domain/task/metadata"
import type { BoardSession } from "../../types"

type HelperFailureNotice = {
  sessionID: string
  taskNumber?: number
  role: "intake" | "validator"
  message: string
}

/** `seen` is mutated in place to dedupe across polls. */
export function detectNewHelperFailures(
  sessions: readonly BoardSession[],
  seen: Map<string, string>,
): HelperFailureNotice[] {
  const detected: HelperFailureNotice[] = []
  const liveIDs = new Set<string>()
  for (const session of sessions) {
    const view = kagan(session.metadata)
    if (session.parentID || view.boardTask !== true) continue
    const error = view.helperError
    if (!error) continue
    liveIDs.add(session.id)
    const signature = `${error.role}:${error.message}`
    if (seen.get(session.id) === signature) continue
    seen.set(session.id, signature)
    detected.push({ sessionID: session.id, taskNumber: view.taskNumber, ...error })
  }
  for (const id of seen.keys()) {
    if (!liveIDs.has(id)) seen.delete(id)
  }
  return detected
}

type AwaitingInputNotice = {
  sessionID: string
  taskNumber?: number
  permissionID: string
  title: string
}

/** `seen` is mutated in place to dedupe across polls. */
export function detectNewAwaitingInput(sessions: readonly BoardSession[], seen: Set<string>): AwaitingInputNotice[] {
  const detected: AwaitingInputNotice[] = []
  const liveIDs = new Set<string>()
  for (const session of sessions) {
    const view = kagan(session.metadata)
    if (session.parentID || view.boardTask !== true) continue
    const awaiting = view.awaitingInput
    if (!awaiting) continue
    liveIDs.add(awaiting.id)
    if (seen.has(awaiting.id)) continue
    seen.add(awaiting.id)
    detected.push({
      sessionID: session.id,
      taskNumber: view.taskNumber,
      permissionID: awaiting.id,
      title: awaiting.title,
    })
  }
  for (const id of seen) {
    if (!liveIDs.has(id)) seen.delete(id)
  }
  return detected
}

export function notifyHelperFailures(
  notify: (toast: { variant: "warning"; title: string; message: string }) => void,
  failures: readonly HelperFailureNotice[],
): void {
  for (const failure of failures) {
    const label = failure.role === "intake" ? "Intake" : "Review"
    const ref = failure.taskNumber !== undefined ? `#${failure.taskNumber}` : failure.sessionID
    notify({
      variant: "warning",
      title: "Kagan",
      message: `${label} failed for ${ref} — ${failure.message} — press r to retry`,
    })
  }
}

export function notifyAwaitingInput(
  notify: (toast: { variant: "warning"; title: string; message: string }) => void,
  waits: readonly AwaitingInputNotice[],
): void {
  for (const wait of waits) {
    const ref = wait.taskNumber !== undefined ? `#${wait.taskNumber}` : wait.sessionID
    notify({ variant: "warning", title: "Kagan", message: `${ref} waiting on you — ${wait.title}` })
  }
}
