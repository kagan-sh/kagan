import { columnMoveDenyReason, countInProgressForMove, inProgressCap } from "../../../domain/task/policy"
import type { ColumnType } from "../../../domain/task/types"
import { setOrder } from "../../session/preferences"
import type { BoardSession } from "../../types"
import { adjacentColumn, firstSessionID, flatNavIDs, nextSessionID } from "./navigation"
import type { StoreState } from "./state"

export type { StoreState } from "./state"

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
