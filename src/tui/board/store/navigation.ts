import { COLUMNS, type ColumnType } from "../../../domain/task/types"
import type { BoardCard } from "../../types"

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

export function firstSessionID(columns: Record<ColumnType, BoardCard[]>, column: ColumnType): string | undefined {
  return flatNavIDs(columns[column])[0]
}

export function nextSessionID(
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
