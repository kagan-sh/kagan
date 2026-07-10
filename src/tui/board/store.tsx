import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import { createMemo, createSignal } from "solid-js"
import type { Event, SessionStatus } from "@opencode-ai/sdk/v2"
import { listSessions, moveSession } from "../session/tasks"
import {
  columnMoveDenyReason,
  countInProgressForMove,
  inProgressCap,
  needsHuman,
  sendBackStopThreshold,
  squashMerge,
} from "../../domain/task/policy"
import { getStatus, kagan } from "../../domain/task/metadata"
import { commandInTaskScope, commandPlan, configuredScopes } from "../../domain/task/commands"
import { deleteSession } from "../tasks"
import { getFilter, getOrder, setFilter as persistFilter, setOrder } from "../session/preferences"
import { COLUMNS, type ColumnType } from "../../domain/task/types"
import { ROUTE, type BoardCard, type BoardSession } from "../types"
import type { UpdateStatus } from "../updates"

const TASK_NUMBER_QUERY = /^#(\d+)$/

function sessionMatchesFilter(session: BoardSession, query: string): boolean {
  const taskNumberQuery = TASK_NUMBER_QUERY.exec(query)
  if (taskNumberQuery) return kagan(session.metadata).taskNumber === Number(taskNumberQuery[1])
  return session.title.toLowerCase().includes(query) || session.slug.toLowerCase().includes(query)
}

function filterSessions(sessions: readonly BoardSession[], filter: string): BoardSession[] {
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

// The project-scoped session.list route (session/session.ts's listByProject, unlike the
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
    ...inOrder.sort((left, right) => orderIndex.get(left.id)! - orderIndex.get(right.id)!),
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

function groupCardsByColumn(
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

function adjacentColumn(column: ColumnType, direction: 1 | -1): ColumnType | undefined {
  const index = COLUMNS.indexOf(column)
  const nextIndex = index + direction
  if (nextIndex < 0 || nextIndex >= COLUMNS.length) return undefined
  return COLUMNS[nextIndex]
}

function flatNavIDs(cards: readonly BoardCard[]): string[] {
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

type BoardNotice = TuiToast & { key: string }

function noticeDuration(toast: TuiToast): number {
  return toast.duration ?? (toast.variant === "error" ? 10000 : 5000)
}

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

function reconcileOrders(
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

export function createBoardStore(api: TuiPluginApi, options?: Record<string, unknown>) {
  const squash = squashMerge(options)
  const setupCommands = commandPlan(options, "setup")
  const checkCommands = commandPlan(options, "check")
  const setup = setupCommands.map((command) => command.command).join(" && ") || undefined
  const check = checkCommands.map((command) => command.command).join(" && ") || undefined
  const scopes = configuredScopes(options)
  const sendBackThreshold = sendBackStopThreshold(options)
  const [sessions, setSessions] = createSignal<BoardSession[]>([])
  const [updateStatus, setUpdateStatus] = createSignal<UpdateStatus>()
  const [selectedID, setSelectedID] = createSignal<string | undefined>()
  const [selectedColumn, setSelectedColumn] = createSignal<ColumnType>("backlog")
  const [filter, setFilterSignal] = createSignal(getFilter(api))
  const [orders, setOrders] = createSignal<Record<ColumnType, readonly string[]>>({
    backlog: [],
    in_progress: [],
    review: [],
    done: [],
  })

  const filteredSessions = createMemo(() => filterSessions(sessions(), filter()))
  const columns = createMemo(() => groupCardsByColumn(filteredSessions(), orders()))

  // Task and board-action feedback renders through the Notice overlay: OpenCode mounts <Toast/>
  // only on home/session routes. Update status is separate persistent footer state.
  const [notices, setNotices] = createSignal<BoardNotice[]>([])
  const noticeTimers = new Map<string, ReturnType<typeof setTimeout>>()
  let noticeSeq = 0
  const NOTICE_CAP = 3

  const clearNoticeTimer = (key: string) => {
    const timer = noticeTimers.get(key)
    if (timer) clearTimeout(timer)
    noticeTimers.delete(key)
  }

  const dismissNotice = (key: string) => {
    clearNoticeTimer(key)
    setNotices((current) => current.filter((notice) => notice.key !== key))
  }

  const notify = (toast: TuiToast) => {
    const key = `notice-${++noticeSeq}`
    setNotices((current) => {
      const next = [...current, { ...toast, key }]
      while (next.length > NOTICE_CAP) clearNoticeTimer(next.shift()!.key)
      return next
    })
    noticeTimers.set(
      key,
      setTimeout(() => dismissNotice(key), noticeDuration(toast)),
    )
  }

  const toastError = (message: string) => {
    notify({ variant: "error", title: "Kagan", message })
  }

  const runWithToast = async <T,>(fn: () => Promise<T>): Promise<T | undefined> => {
    try {
      return await fn()
    } catch (error) {
      toastError(error instanceof Error ? error.message : String(error))
      return undefined
    }
  }

  let refreshStarted = 0
  let refreshCompleted = 0
  const helperFailuresSeen = new Map<string, string>()
  const awaitingInputSeen = new Set<string>()

  const refresh = async () => {
    const generation = ++refreshStarted
    const result = await runWithToast(async () => {
      const list = await listSessions(api)
      return list.map((session) => ({
        ...session,
        kaganStatus: getStatus(session.metadata),
      }))
    })
    if (result === undefined) return
    if (generation <= refreshCompleted) return
    refreshCompleted = generation
    setSessions(result)
    for (const failure of detectNewHelperFailures(result, helperFailuresSeen)) {
      const label = failure.role === "intake" ? "Intake" : "Review"
      const ref = failure.taskNumber !== undefined ? `#${failure.taskNumber}` : failure.sessionID
      notify({
        variant: "warning",
        title: "Kagan",
        message: `${label} failed for ${ref} — ${failure.message} — press r to retry`,
      })
    }
    for (const wait of detectNewAwaitingInput(result, awaitingInputSeen)) {
      const ref = wait.taskNumber !== undefined ? `#${wait.taskNumber}` : wait.sessionID
      notify({ variant: "warning", title: "Kagan", message: `${ref} waiting on you — ${wait.title}` })
    }
    const loaded: Record<ColumnType, readonly string[]> = {
      backlog: getOrder(api, "backlog"),
      in_progress: getOrder(api, "in_progress"),
      review: getOrder(api, "review"),
      done: getOrder(api, "done"),
    }
    const reconciled = reconcileOrders(result, loaded)
    for (const column of COLUMNS) {
      if (reconciled[column].join() !== loaded[column].join()) {
        setOrder(api, column, reconciled[column])
      }
    }
    setOrders(reconciled)
    setFilterSignal(getFilter(api))
  }

  const selectedSession = (): BoardSession | undefined => {
    const id = selectedID()
    if (!id) return undefined
    return sessions().find((item) => item.id === id)
  }

  const select = (column: ColumnType, id: string | undefined) => {
    setSelectedColumn(column)
    const nav = flatNavIDs(columns()[column])
    const valid = id && nav.includes(id) ? id : nav[0]
    setSelectedID(valid)
  }

  const selectNext = () => {
    const result = nextSessionID(columns(), selectedColumn(), selectedID(), 1)
    if (!result) return
    select(result.column, result.id)
  }

  const selectPrevious = () => {
    const result = nextSessionID(columns(), selectedColumn(), selectedID(), -1)
    if (!result) return
    select(result.column, result.id)
  }

  const selectNextColumn = () => {
    const next = adjacentColumn(selectedColumn(), 1)
    if (!next) return
    setSelectedColumn(next)
    const id = firstSessionID(columns(), next)
    if (id) setSelectedID(id)
  }

  const selectPrevColumn = () => {
    const previous = adjacentColumn(selectedColumn(), -1)
    if (!previous) return
    setSelectedColumn(previous)
    const id = firstSessionID(columns(), previous)
    if (id) setSelectedID(id)
  }

  const selectEdge = (edge: "first" | "last") => {
    const nav = flatNavIDs(columns()[selectedColumn()])
    const id = edge === "first" ? nav[0] : nav.at(-1)
    if (id) select(selectedColumn(), id)
  }
  const selectFirst = () => selectEdge("first")
  const selectLast = () => selectEdge("last")

  const reorder = (direction: 1 | -1) => {
    const session = selectedSession()
    if (!session || session.parentID) return
    const column = session.kaganStatus
    const order = [...orders()[column]]
    const index = order.indexOf(session.id)
    const swapIndex = index + direction
    if (index === -1 || swapIndex < 0 || swapIndex >= order.length) return
    const neighbor = order[swapIndex]!
    order[swapIndex] = order[index]!
    order[index] = neighbor
    setOrder(api, column, order)
    setOrders((current) => ({ ...current, [column]: order }))
  }

  const [sessionStatuses, setSessionStatuses] = createSignal<Record<string, SessionStatus["type"]>>({})
  const setSessionStatus = (sessionID: string, status: SessionStatus["type"]) => {
    setSessionStatuses((current) => ({ ...current, [sessionID]: status }))
  }
  const sessionStatus = (sessionID: string): SessionStatus["type"] | undefined => sessionStatuses()[sessionID]

  const moveDenyReason = (status: ColumnType, session: BoardSession): string | undefined => {
    const moveCtx = {
      inProgressCount: countInProgressForMove(
        sessions().map((item) => ({ id: item.id, parentID: item.parentID, status: item.kaganStatus })),
        session.id,
        session.kaganStatus,
      ),
      source: session.kaganStatus,
      cap: inProgressCap(options),
    }
    return columnMoveDenyReason(status, session.metadata, moveCtx)
  }

  const moveTo = async (status: ColumnType) => {
    const session = selectedSession()
    if (!session) return
    const id = session.id
    if (session.parentID) {
      toastError("Subtasks cannot be moved between columns")
      return
    }
    const source = session.kaganStatus
    const reason = moveDenyReason(status, session)
    if (reason) {
      toastError(reason)
      return
    }
    const moved = await runWithToast(async () => {
      await moveSession(api, id, status)
      return true
    })
    if (!moved) return
    setOrder(
      api,
      source,
      getOrder(api, source).filter((item) => item !== id),
    )
    setOrder(api, status, [...getOrder(api, status), id])
    setSelectedColumn(status)
    await refresh()
  }

  const moveByDirection = async (direction: 1 | -1) => {
    const session = selectedSession()
    if (!session) return
    const target = adjacentColumn(session.kaganStatus, direction)
    if (!target) return
    await moveTo(target)
  }
  const moveNext = () => moveByDirection(1)
  const movePrevious = () => moveByDirection(-1)

  const deleteSelected = async () => {
    const id = selectedID()
    if (!id) return
    const session = selectedSession()
    const column = selectedColumn()
    const nav = flatNavIDs(columns()[column])
    const index = nav.indexOf(id)
    const nextID = nav[index + 1] ?? nav[index - 1]

    const deleted = await runWithToast(async () => {
      await deleteSession(api, id)
      return true
    })
    if (!deleted) return

    if (!session?.parentID) {
      setOrder(
        api,
        column,
        getOrder(api, column).filter((item) => item !== id),
      )
    }
    await refresh()

    if (nextID && sessions().some((item) => item.id === nextID)) {
      select(column, nextID)
      return
    }

    const sameColumn = firstSessionID(columns(), column)
    if (sameColumn) {
      select(column, sameColumn)
      return
    }

    for (const status of COLUMNS) {
      const first = firstSessionID(columns(), status)
      if (first) {
        select(status, first)
        return
      }
    }

    setSelectedID(undefined)
  }

  return {
    sessions,
    selected: selectedID,
    selectedSession,
    selectedColumn,
    filter,
    squashMerge: squash,
    setupCommand: setup,
    checkCommand: check,
    setupCommands,
    checkCommands,
    configuredScopes: scopes,
    sendBackStopThreshold: sendBackThreshold,
    inProgressCap: inProgressCap(options),
    columns,
    notices,
    notify,
    updateStatus,
    setUpdateStatus,
    select,
    selectNext,
    selectPrevious,
    selectNextColumn,
    selectPrevColumn,
    selectFirst,
    selectLast,
    reorder,
    sessionStatus,
    setSessionStatus,
    moveNext,
    movePrevious,
    moveTo,
    moveDenyReason,
    deleteSelected,
    refresh,
    setFilter(value: string) {
      setFilterSignal(value)
      persistFilter(api, value)
      select(selectedColumn(), selectedID())
    },
  }
}

export const SESSION_EVENT_DEBOUNCE_MS = 1000

export function createSessionEventSubscription(api: TuiPluginApi, refresh: () => Promise<void>): () => void {
  const types: Array<Event["type"]> = ["session.created", "session.updated", "session.idle", "session.deleted"]
  let timer: ReturnType<typeof setTimeout> | undefined
  const scheduleRefresh = () => {
    if (api.route.current.name !== ROUTE) return
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => {
      timer = undefined
      refresh()
    }, SESSION_EVENT_DEBOUNCE_MS)
  }
  const disposers = types.map((type) => api.event.on(type, scheduleRefresh))
  return () => {
    if (timer) clearTimeout(timer)
    disposers.forEach((dispose) => dispose())
  }
}

// Independent of the debounced session refresh above: session.status fires far more often than
// the session lifecycle events that trigger a refetch, and the working indicator needs to react
// immediately rather than wait for the next debounced poll.
export function createSessionStatusSubscription(
  api: TuiPluginApi,
  setStatus: (sessionID: string, status: SessionStatus["type"]) => void,
): () => void {
  return api.event.on("session.status", (event) => setStatus(event.properties.sessionID, event.properties.status.type))
}
