import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import { columnMoveDenyReason, countInProgressForMove, inProgressCap } from "../../../domain/task/policy"
import { COLUMNS, type ColumnType } from "../../../domain/task/types"
import { setOrder } from "../../session/preferences"
import type { BoardCard, BoardSession } from "../../types"

type SignalRead<T> = () => T
type SignalWrite<T> = (value: T | ((prev: T) => T)) => void

export type StoreState = {
  api: TuiPluginApi
  options?: Record<string, unknown>
  sessions: SignalRead<BoardSession[]>
  setSessions: SignalWrite<BoardSession[]>
  selectedID: SignalRead<string | undefined>
  setSelectedID: SignalWrite<string | undefined>
  selectedColumn: SignalRead<ColumnType>
  setSelectedColumn: SignalWrite<ColumnType>
  filter: SignalRead<string>
  setFilterSignal: SignalWrite<string>
  orders: SignalRead<Record<ColumnType, readonly string[]>>
  setOrders: SignalWrite<Record<ColumnType, readonly string[]>>
  columns: SignalRead<Record<ColumnType, BoardCard[]>>
  notify: (toast: TuiToast) => void
  toastError: (message: string) => void
  runWithToast: <T>(fn: () => Promise<T>) => Promise<T | undefined>
  refreshState: {
    started: number
    completed: number
    helperFailuresSeen: Map<string, string>
    awaitingPermissionsSeen: Set<string>
  }
}

export function adjacentColumn(column: ColumnType, direction: 1 | -1): ColumnType | undefined {
  const index = COLUMNS.indexOf(column)
  const nextIndex = index + direction
  if (nextIndex < 0 || nextIndex >= COLUMNS.length) return undefined
  return COLUMNS[nextIndex]
}

export function flatNavIDs(cards: readonly BoardCard[]): string[] {
  const ids: string[] = []
  for (const card of cards) {
    ids.push(card.session.id)
    for (const child of card.children) {
      ids.push(child.id)
    }
  }
  return ids
}

function firstSessionID(columns: Record<ColumnType, BoardCard[]>, column: ColumnType): string | undefined {
  return flatNavIDs(columns[column])[0]
}

function nextSessionID(
  columns: Record<ColumnType, BoardCard[]>,
  column: ColumnType,
  currentID: string | undefined,
  direction: 1 | -1,
): { column: ColumnType; id: string } | undefined {
  const list = flatNavIDs(columns[column])
  if (list.length === 0) return undefined
  const first = list[0]
  if (!first) return undefined
  if (!currentID) return { column, id: first }
  const index = list.indexOf(currentID)
  if (index === -1) return { column, id: first }
  const nextIndex = index + direction
  if (nextIndex < 0 || nextIndex >= list.length) return undefined
  const next = list[nextIndex]
  if (!next) return undefined
  return { column, id: next }
}

export function selectedSession(s: StoreState): BoardSession | undefined {
  const id = s.selectedID()
  if (!id) return undefined
  return s.sessions().find((item) => item.id === id)
}

export function select(s: StoreState, column: ColumnType, id: string | undefined) {
  s.setSelectedColumn(column)
  const nav = flatNavIDs(s.columns()[column])
  const valid = id && nav.includes(id) ? id : nav[0]
  s.setSelectedID(valid)
}

export function selectStep(s: StoreState, direction: 1 | -1) {
  const result = nextSessionID(s.columns(), s.selectedColumn(), s.selectedID(), direction)
  if (!result) return
  select(s, result.column, result.id)
}

export function selectColumnStep(s: StoreState, direction: 1 | -1) {
  const target = adjacentColumn(s.selectedColumn(), direction)
  if (!target) return
  s.setSelectedColumn(target)
  const id = firstSessionID(s.columns(), target)
  if (id) s.setSelectedID(id)
}

export function selectEdge(s: StoreState, edge: "first" | "last") {
  const nav = flatNavIDs(s.columns()[s.selectedColumn()])
  const id = edge === "first" ? nav[0] : nav.at(-1)
  if (id) select(s, s.selectedColumn(), id)
}

export function reorder(s: StoreState, direction: 1 | -1) {
  const session = selectedSession(s)
  if (!session || session.parentID) return
  const column = session.kaganStatus
  const order = [...s.orders()[column]]
  const index = order.indexOf(session.id)
  const swapIndex = index + direction
  if (index === -1 || swapIndex < 0 || swapIndex >= order.length) return
  const neighbor = order[swapIndex]
  const current = order[index]
  if (neighbor === undefined || current === undefined) return
  order[swapIndex] = current
  order[index] = neighbor
  setOrder(s.api, column, order)
  s.setOrders((existing) => ({ ...existing, [column]: order }))
}

export function moveDenyReason(s: StoreState, status: ColumnType, session: BoardSession): string | undefined {
  const moveCtx = {
    inProgressCount: countInProgressForMove(
      s.sessions().map((item) => ({ id: item.id, parentID: item.parentID, status: item.kaganStatus })),
      session.id,
      session.kaganStatus,
    ),
    source: session.kaganStatus,
    cap: inProgressCap(s.options),
  }
  return columnMoveDenyReason(status, session.metadata, moveCtx)
}

export function selectAfterDelete(s: StoreState, column: ColumnType, nextID: string | undefined) {
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
