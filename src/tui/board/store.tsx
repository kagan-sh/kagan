import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createMemo, createSignal } from "solid-js"
import type { Event, SessionStatus } from "@opencode-ai/sdk/v2"
import { commandPlan, configuredScopes } from "../../domain/task/commands"
import { inProgressCap, sendBackStopThreshold, squashMerge } from "../../domain/task/policy"
import { getFilter, setFilter as persistFilter } from "../session/preferences"
import type { ColumnType } from "../../domain/task/types"
import { ROUTE, type BoardSession } from "../types"
import type { UpdateStatus } from "../updates/check"
import { filterSessions, groupCardsByColumn } from "./store/sessions"
import { createNotices } from "./store/notices"
import {
  moveDenyReason,
  reorder,
  select,
  selectColumnStep,
  selectEdge,
  selectStep,
  selectedSession,
  type StoreState,
} from "./store/selection"
import { deleteSelected, moveByDirection, moveTo, refresh } from "./store/refresh"

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
    refreshState: { started: 0, completed: 0, helperFailuresSeen: new Map(), awaitingPermissionsSeen: new Set() },
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

export type BoardStore = ReturnType<typeof createBoardStore>

export const SESSION_EVENT_DEBOUNCE_MS = 1000

export function createSessionEventSubscription(api: TuiPluginApi, refreshFn: () => Promise<void>): () => void {
  const types: Array<Event["type"]> = ["session.created", "session.updated", "session.idle", "session.deleted"]
  let timer: ReturnType<typeof setTimeout> | undefined
  const scheduleRefresh = () => {
    if (api.route.current.name !== ROUTE) return
    if (timer) clearTimeout(timer)
    timer = setTimeout(() => {
      timer = undefined
      refreshFn()
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
