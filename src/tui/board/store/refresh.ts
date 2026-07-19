import { kagan, getStatus } from "../../../domain/task/metadata"
import { COLUMNS, type ColumnType } from "../../../domain/task/types"
import { listSessions, moveSession } from "../../session/tasks"
import { deleteSession } from "../../tasks"
import { getFilter, getOrder, setOrder } from "../../session/preferences"
import type { BoardSession } from "../../types"
import type { StoreState } from "./selection"
import { reconcileOrders } from "./sessions"
import { adjacentColumn, flatNavIDs, moveDenyReason, selectAfterDelete, selectedSession } from "./selection"

type HelperFailureNotice = {
  sessionID: string
  taskNumber?: number
  role: "intake" | "validator"
  message: string
}

/** `seen` is mutated in place to dedupe across polls. */
function detectNewHelperFailures(sessions: readonly BoardSession[], seen: Map<string, string>): HelperFailureNotice[] {
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
function detectNewAwaitingInput(sessions: readonly BoardSession[], seen: Set<string>): AwaitingInputNotice[] {
  const detected: AwaitingInputNotice[] = []
  const liveIDs = new Set<string>()
  for (const session of sessions) {
    const view = kagan(session.metadata)
    if (session.parentID || view.boardTask !== true) continue
    for (const awaiting of view.awaitingPermissions ?? []) {
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
  }
  for (const id of seen) {
    if (!liveIDs.has(id)) seen.delete(id)
  }
  return detected
}

function notifyHelperFailures(
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

function notifyAwaitingInput(
  notify: (toast: { variant: "warning"; title: string; message: string }) => void,
  waits: readonly AwaitingInputNotice[],
): void {
  for (const wait of waits) {
    const ref = wait.taskNumber !== undefined ? `#${wait.taskNumber}` : wait.sessionID
    notify({ variant: "warning", title: "Kagan", message: `${ref} waiting on you — ${wait.title} — press p` })
  }
}

async function fetchBoardSessions(s: StoreState): Promise<BoardSession[] | undefined> {
  const list = await s.runWithToast(async () => {
    const sessions = await listSessions(s.api)
    return sessions.map((session) => ({ ...session, kaganStatus: getStatus(session.metadata) }))
  })
  return list
}

function applyDetectionNotices(s: StoreState, sessions: readonly BoardSession[]) {
  notifyHelperFailures(s.notify, detectNewHelperFailures(sessions, s.refreshState.helperFailuresSeen))
  notifyAwaitingInput(s.notify, detectNewAwaitingInput(sessions, s.refreshState.awaitingPermissionsSeen))
}

function loadColumnOrders(s: StoreState): Record<ColumnType, readonly string[]> {
  return {
    backlog: getOrder(s.api, "backlog"),
    in_progress: getOrder(s.api, "in_progress"),
    review: getOrder(s.api, "review"),
    done: getOrder(s.api, "done"),
  }
}

function persistReconciledOrders(
  s: StoreState,
  reconciled: Record<ColumnType, string[]>,
  loaded: Record<ColumnType, readonly string[]>,
) {
  for (const column of COLUMNS) {
    if (reconciled[column].join() !== loaded[column].join()) setOrder(s.api, column, reconciled[column])
  }
}

export async function refresh(s: StoreState) {
  const rs = s.refreshState
  const generation = ++rs.started
  const result = await fetchBoardSessions(s)
  if (result === undefined) return
  if (generation <= rs.completed) return
  rs.completed = generation
  s.setSessions(result)
  applyDetectionNotices(s, result)
  const loaded = loadColumnOrders(s)
  const reconciled = reconcileOrders(result, loaded)
  persistReconciledOrders(s, reconciled, loaded)
  s.setOrders(reconciled)
  s.setFilterSignal(getFilter(s.api))
}

export async function moveTo(s: StoreState, status: ColumnType) {
  const session = selectedSession(s)
  if (!session) return
  const id = session.id
  if (session.parentID) {
    s.toastError("Subtasks cannot be moved between columns")
    return
  }
  const source = session.kaganStatus
  const reason = moveDenyReason(s, status, session)
  if (reason) {
    s.toastError(reason)
    return
  }
  const moved = await s.runWithToast(async () => {
    await moveSession(s.api, id, status)
    return true
  })
  if (!moved) return
  setOrder(
    s.api,
    source,
    getOrder(s.api, source).filter((item) => item !== id),
  )
  setOrder(s.api, status, [...getOrder(s.api, status), id])
  s.setSelectedColumn(status)
  await refresh(s)
}

export async function moveByDirection(s: StoreState, direction: 1 | -1) {
  const session = selectedSession(s)
  if (!session) return
  const target = adjacentColumn(session.kaganStatus, direction)
  if (!target) return
  await moveTo(s, target)
}

export async function deleteSelected(s: StoreState) {
  const id = s.selectedID()
  if (!id) return
  const session = selectedSession(s)
  const column = s.selectedColumn()
  const nav = flatNavIDs(s.columns()[column])
  const index = nav.indexOf(id)
  const nextID = nav[index + 1] ?? nav[index - 1]

  const deleted = await s.runWithToast(async () => {
    await deleteSession(s.api, id)
    return true
  })
  if (!deleted) return

  if (!session?.parentID) {
    setOrder(
      s.api,
      column,
      getOrder(s.api, column).filter((item) => item !== id),
    )
  }
  await refresh(s)
  selectAfterDelete(s, column, nextID)
}
