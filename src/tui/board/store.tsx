import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import { type Accessor, type Setter, createMemo, createSignal } from "solid-js"
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
import { commandPlan, configuredScopes } from "../../domain/task/commands"
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

const NOTICE_CAP = 3

// context: OpenCode mounts <Toast/> only on home/session routes, so board feedback renders through this Notice overlay state rather than api.ui.toast.
function createNotices() {
  const [notices, setNotices] = createSignal<BoardNotice[]>([])
  const noticeTimers = new Map<string, ReturnType<typeof setTimeout>>()
  let noticeSeq = 0

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
      while (next.length > NOTICE_CAP) {
        const expired = next.shift()
        if (expired) clearNoticeTimer(expired.key)
      }
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

  return { notices, notify, toastError, runWithToast }
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

type StoreState = {
  api: TuiPluginApi
  options?: Record<string, unknown>
  sessions: Accessor<BoardSession[]>
  setSessions: Setter<BoardSession[]>
  selectedID: Accessor<string | undefined>
  setSelectedID: Setter<string | undefined>
  selectedColumn: Accessor<ColumnType>
  setSelectedColumn: Setter<ColumnType>
  filter: Accessor<string>
  setFilterSignal: Setter<string>
  orders: Accessor<Record<ColumnType, readonly string[]>>
  setOrders: Setter<Record<ColumnType, readonly string[]>>
  columns: Accessor<Record<ColumnType, BoardCard[]>>
  notify: (toast: TuiToast) => void
  toastError: (message: string) => void
  runWithToast: <T>(fn: () => Promise<T>) => Promise<T | undefined>
  refreshState: {
    started: number
    completed: number
    helperFailuresSeen: Map<string, string>
    awaitingInputSeen: Set<string>
  }
}

function selectedSession(s: StoreState): BoardSession | undefined {
  const id = s.selectedID()
  if (!id) return undefined
  return s.sessions().find((item) => item.id === id)
}

function select(s: StoreState, column: ColumnType, id: string | undefined) {
  s.setSelectedColumn(column)
  const nav = flatNavIDs(s.columns()[column])
  const valid = id && nav.includes(id) ? id : nav[0]
  s.setSelectedID(valid)
}

function selectStep(s: StoreState, direction: 1 | -1) {
  const result = nextSessionID(s.columns(), s.selectedColumn(), s.selectedID(), direction)
  if (!result) return
  select(s, result.column, result.id)
}

function selectColumnStep(s: StoreState, direction: 1 | -1) {
  const target = adjacentColumn(s.selectedColumn(), direction)
  if (!target) return
  s.setSelectedColumn(target)
  const id = firstSessionID(s.columns(), target)
  if (id) s.setSelectedID(id)
}

function selectEdge(s: StoreState, edge: "first" | "last") {
  const nav = flatNavIDs(s.columns()[s.selectedColumn()])
  const id = edge === "first" ? nav[0] : nav.at(-1)
  if (id) select(s, s.selectedColumn(), id)
}

function reorder(s: StoreState, direction: 1 | -1) {
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

function moveDenyReason(s: StoreState, status: ColumnType, session: BoardSession): string | undefined {
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

async function refresh(s: StoreState) {
  const rs = s.refreshState
  const generation = ++rs.started
  const result = await s.runWithToast(async () => {
    const list = await listSessions(s.api)
    return list.map((session) => ({ ...session, kaganStatus: getStatus(session.metadata) }))
  })
  if (result === undefined) return
  if (generation <= rs.completed) return
  rs.completed = generation
  s.setSessions(result)
  for (const failure of detectNewHelperFailures(result, rs.helperFailuresSeen)) {
    const label = failure.role === "intake" ? "Intake" : "Review"
    const ref = failure.taskNumber !== undefined ? `#${failure.taskNumber}` : failure.sessionID
    s.notify({
      variant: "warning",
      title: "Kagan",
      message: `${label} failed for ${ref} — ${failure.message} — press r to retry`,
    })
  }
  for (const wait of detectNewAwaitingInput(result, rs.awaitingInputSeen)) {
    const ref = wait.taskNumber !== undefined ? `#${wait.taskNumber}` : wait.sessionID
    s.notify({ variant: "warning", title: "Kagan", message: `${ref} waiting on you — ${wait.title}` })
  }
  const loaded: Record<ColumnType, readonly string[]> = {
    backlog: getOrder(s.api, "backlog"),
    in_progress: getOrder(s.api, "in_progress"),
    review: getOrder(s.api, "review"),
    done: getOrder(s.api, "done"),
  }
  const reconciled = reconcileOrders(result, loaded)
  for (const column of COLUMNS) {
    if (reconciled[column].join() !== loaded[column].join()) setOrder(s.api, column, reconciled[column])
  }
  s.setOrders(reconciled)
  s.setFilterSignal(getFilter(s.api))
}

async function moveTo(s: StoreState, status: ColumnType) {
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

async function moveByDirection(s: StoreState, direction: 1 | -1) {
  const session = selectedSession(s)
  if (!session) return
  const target = adjacentColumn(session.kaganStatus, direction)
  if (!target) return
  await moveTo(s, target)
}

async function deleteSelected(s: StoreState) {
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
  const { notices, notify, toastError, runWithToast } = createNotices()
  const [sessionStatuses, setSessionStatuses] = createSignal<Record<string, SessionStatus["type"]>>({})

  const s: StoreState = {
    api,
    options,
    sessions,
    setSessions,
    selectedID,
    setSelectedID,
    selectedColumn,
    setSelectedColumn,
    filter,
    setFilterSignal,
    orders,
    setOrders,
    columns,
    notify,
    toastError,
    runWithToast,
    refreshState: { started: 0, completed: 0, helperFailuresSeen: new Map(), awaitingInputSeen: new Set() },
  }

  return {
    sessions,
    selected: selectedID,
    selectedSession: () => selectedSession(s),
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
    select: (column: ColumnType, id: string | undefined) => select(s, column, id),
    selectNext: () => selectStep(s, 1),
    selectPrevious: () => selectStep(s, -1),
    selectNextColumn: () => selectColumnStep(s, 1),
    selectPrevColumn: () => selectColumnStep(s, -1),
    selectFirst: () => selectEdge(s, "first"),
    selectLast: () => selectEdge(s, "last"),
    reorder: (direction: 1 | -1) => reorder(s, direction),
    sessionStatus: (sessionID: string): SessionStatus["type"] | undefined => sessionStatuses()[sessionID],
    setSessionStatus: (sessionID: string, status: SessionStatus["type"]) =>
      setSessionStatuses((current) => ({ ...current, [sessionID]: status })),
    moveNext: () => moveByDirection(s, 1),
    movePrevious: () => moveByDirection(s, -1),
    moveTo: (status: ColumnType) => moveTo(s, status),
    moveDenyReason: (status: ColumnType, session: BoardSession) => moveDenyReason(s, status, session),
    deleteSelected: () => deleteSelected(s),
    refresh: () => refresh(s),
    setFilter(value: string) {
      setFilterSignal(value)
      persistFilter(api, value)
      select(s, selectedColumn(), selectedID())
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

// context: independent of the debounced session refresh above: session.status fires far more often than
// the session lifecycle events that trigger a refetch, and the working indicator needs to react
// immediately rather than wait for the next debounced poll.
export function createSessionStatusSubscription(
  api: TuiPluginApi,
  setStatus: (sessionID: string, status: SessionStatus["type"]) => void,
): () => void {
  return api.event.on("session.status", (event) => setStatus(event.properties.sessionID, event.properties.status.type))
}
