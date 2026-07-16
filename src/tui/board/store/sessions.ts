import { needsHuman } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import { COLUMNS, type ColumnType } from "../../../domain/task/types"
import type { BoardCard, BoardSession } from "../../types"

const TASK_NUMBER_QUERY = /^#(\d+)$/

function sessionMatchesFilter(session: BoardSession, query: string): boolean {
  const taskNumberQuery = TASK_NUMBER_QUERY.exec(query)
  if (taskNumberQuery) return kagan(session.metadata).taskNumber === Number(taskNumberQuery[1])
  return session.title.toLowerCase().includes(query) || session.slug.toLowerCase().includes(query)
}

export function filterSessions(sessions: readonly BoardSession[], filter: string): BoardSession[] {
  const query = filter.trim().toLowerCase()
  if (!query) return [...sessions]

  const byID = new Map(sessions.map((session) => [session.id, session]))
  const included = new Set<string>()

  for (const session of sessions) {
    if (!sessionMatchesFilter(session, query)) continue
    included.add(session.id)
    let parentID = session.parentID
    while (parentID) {
      included.add(parentID)
      parentID = byID.get(parentID)?.parentID
    }
  }

  return sessions.filter((session) => included.has(session.id))
}

// context: the project-scoped session.list route (session/session.ts's listByProject, unlike the
// Experimental global list) applies no archived filter, so archived cards would otherwise
// linger on the board after archiveSession stamps time.archived.
function rootSessions(sessions: readonly BoardSession[]): BoardSession[] {
  return sessions.filter(
    (session) => !session.parentID && kagan(session.metadata).boardTask === true && session.time.archived === undefined,
  )
}

function attachChildren(roots: readonly BoardSession[], sessions: readonly BoardSession[]): BoardCard[] {
  const byParent = new Map<string, BoardSession[]>()
  for (const session of sessions) {
    if (!session.parentID) continue
    const list = byParent.get(session.parentID) ?? []
    list.push(session)
    byParent.set(session.parentID, list)
  }
  for (const list of byParent.values()) {
    list.sort((left, right) => right.time.updated - left.time.updated)
  }
  return roots.map((session) => ({
    session,
    children: byParent.get(session.id) ?? [],
  }))
}

function sortSessionsByOrder(sessions: readonly BoardSession[], order: readonly string[]): BoardSession[] {
  const orderIndex = new Map(order.map((id, index) => [id, index]))
  const inOrder = sessions.filter((session) => orderIndex.has(session.id))
  const remaining = sessions.filter((session) => !orderIndex.has(session.id))
  return [
    ...inOrder.sort((left, right) => (orderIndex.get(left.id) ?? 0) - (orderIndex.get(right.id) ?? 0)),
    ...remaining.sort((left, right) => right.time.updated - left.time.updated),
  ]
}

function sortNeedsHumanFirst(sessions: readonly BoardSession[]): BoardSession[] {
  const needsYou = sessions.filter((session) => needsHuman(session.kaganStatus, session.metadata))
  const rest = sessions.filter((session) => !needsHuman(session.kaganStatus, session.metadata))
  return [...needsYou, ...rest]
}

function groupSessionsByColumn(
  sessions: readonly BoardSession[],
  orders: Record<ColumnType, readonly string[]>,
): Record<ColumnType, BoardSession[]> {
  const grouped: Record<ColumnType, BoardSession[]> = {
    backlog: [],
    in_progress: [],
    review: [],
    done: [],
  }
  for (const session of rootSessions(sessions)) {
    grouped[session.kaganStatus].push(session)
  }
  for (const column of COLUMNS) {
    grouped[column] = sortNeedsHumanFirst(sortSessionsByOrder(grouped[column], orders[column]))
  }
  return grouped
}

export function groupCardsByColumn(
  sessions: readonly BoardSession[],
  orders: Record<ColumnType, readonly string[]>,
): Record<ColumnType, BoardCard[]> {
  const grouped = groupSessionsByColumn(sessions, orders)
  return {
    backlog: attachChildren(grouped.backlog, sessions),
    in_progress: attachChildren(grouped.in_progress, sessions),
    review: attachChildren(grouped.review, sessions),
    done: attachChildren(grouped.done, sessions),
  }
}

export function reconcileOrders(
  sessions: readonly BoardSession[],
  orders: Record<ColumnType, readonly string[]>,
): Record<ColumnType, string[]> {
  const result = Object.fromEntries(COLUMNS.map((column) => [column, [...orders[column]]])) as Record<
    ColumnType,
    string[]
  >
  const roots = rootSessions(sessions)
  const statusByID = new Map(roots.map((session) => [session.id, session.kaganStatus]))

  for (const column of COLUMNS) {
    result[column] = result[column].filter((id) => statusByID.get(id) === column)
  }
  for (const session of roots) {
    const column = session.kaganStatus
    if (!result[column].includes(session.id)) {
      result[column].push(session.id)
    }
  }
  return result
}
