import { getStatus } from "../../../domain/task/metadata"
import { COLUMNS, type ColumnType } from "../../../domain/task/types"
import { listSessions, moveSession } from "../../session/tasks"
import { deleteSession } from "../../tasks"
import { getFilter, getOrder, setOrder } from "../../session/preferences"
import type { BoardSession } from "../../types"
import { detectNewAwaitingInput, detectNewHelperFailures, notifyAwaitingInput, notifyHelperFailures } from "./detection"
import { reconcileOrders } from "./sessions"
import { adjacentColumn, firstSessionID, flatNavIDs } from "./navigation"
import { moveDenyReason, select, selectedSession, type StoreState } from "./selection"

async function fetchBoardSessions(s: StoreState): Promise<BoardSession[] | undefined> {
  const list = await s.runWithToast(async () => {
    const sessions = await listSessions(s.api)
    return sessions.map((session) => ({ ...session, kaganStatus: getStatus(session.metadata) }))
  })
  return list
}

function applyDetectionNotices(s: StoreState, sessions: readonly BoardSession[]) {
  notifyHelperFailures(s.notify, detectNewHelperFailures(sessions, s.refreshState.helperFailuresSeen))
  notifyAwaitingInput(s.notify, detectNewAwaitingInput(sessions, s.refreshState.awaitingInputSeen))
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

function selectAfterDelete(s: StoreState, column: ColumnType, nextID: string | undefined) {
  if (nextID && s.sessions().some((item) => item.id === nextID)) {
    select(s, column, nextID)
    return
  }
  const sameColumn = firstSessionID(s.columns(), column)
  if (sameColumn) {
    select(s, column, sameColumn)
    return
  }
  for (const status of COLUMNS) {
    const first = firstSessionID(s.columns(), status)
    if (first) {
      select(s, status, first)
      return
    }
  }
  s.setSelectedID(undefined)
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
