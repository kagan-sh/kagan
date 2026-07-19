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

function columnContaining(columns: Record<ColumnType, BoardCard[]>, id: string | undefined): ColumnType | undefined {
  if (!id) return undefined
  for (const column of COLUMNS) {
    if (flatNavIDs(columns[column]).includes(id)) return column
  }
  return undefined
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

export function ensureSelection(s: StoreState) {
  const columns = s.columns()
  const column = columnContaining(columns, s.selectedID())
  if (column) {
    if (s.selectedColumn() !== column) s.setSelectedColumn(column)
    return
  }
  const fallback = firstSessionID(columns, s.selectedColumn())
  if (fallback) {
    select(s, s.selectedColumn(), fallback)
    return
  }
  for (const status of COLUMNS) {
    const first = firstSessionID(columns, status)
    if (first) {
      select(s, status, first)
      return
    }
  }
  s.setSelectedID(undefined)
}

export function selectStep(s: StoreState, direction: 1 | -1) {
  const id = s.selectedID()
  const column = columnContaining(s.columns(), id) ?? s.selectedColumn()
  const result = nextSessionID(s.columns(), column, id, direction)
  if (!result) return
  select(s, result.column, result.id)
}

function rootNavEntries(columns: Record<ColumnType, BoardCard[]>): { column: ColumnType; id: string }[] {
  const entries: { column: ColumnType; id: string }[] = []
  for (const column of COLUMNS) {
    for (const card of columns[column]) {
      entries.push({ column, id: card.session.id })
    }
  }
  return entries
}

function rootNavIndex(columns: Record<ColumnType, BoardCard[]>, id: string | undefined): number {
  const entries = rootNavEntries(columns)
  if (!id) return -1
  const exact = entries.findIndex((entry) => entry.id === id)
  if (exact !== -1) return exact
  return entries.findIndex((entry) => {
    const card = columns[entry.column].find((item) => item.session.id === entry.id)
    return card?.children.some((child) => child.id === id) ?? false
  })
}

export function selectRootStep(s: StoreState, direction: 1 | -1) {
  const columns = s.columns()
  const entries = rootNavEntries(columns)
  if (entries.length === 0) return
  const index = rootNavIndex(columns, s.selectedID())
  const nextIndex =
    index === -1 ? (direction === 1 ? 0 : entries.length - 1) : (index + direction + entries.length) % entries.length
  const next = entries[nextIndex]
  if (!next) return
  select(s, next.column, next.id)
}

export function selectColumnStep(s: StoreState, direction: 1 | -1) {
  const target = adjacentColumn(s.selectedColumn(), direction)
  if (!target) return
  select(s, target, firstSessionID(s.columns(), target))
}

export function selectEdge(s: StoreState, edge: "first" | "last") {
  const id = s.selectedID()
  const column = columnContaining(s.columns(), id) ?? s.selectedColumn()
  const nav = flatNavIDs(s.columns()[column])
  const next = edge === "first" ? nav[0] : nav.at(-1)
  if (next) select(s, column, next)
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
