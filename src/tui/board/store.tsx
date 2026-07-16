import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Event, SessionStatus } from "@opencode-ai/sdk/v2"
import { commandPlan, configuredScopes } from "../../domain/task/commands"
import { inProgressCap, sendBackStopThreshold, squashMerge } from "../../domain/task/policy"
import { setFilter as persistFilter } from "../session/preferences"
import type { ColumnType } from "../../domain/task/types"
import { ROUTE, type BoardSession } from "../types"
import {
  moveDenyReason,
  reorder,
  select,
  selectColumnStep,
  selectEdge,
  selectStep,
  selectedSession,
} from "./store/selection"
import { deleteSelected, moveByDirection, moveTo, refresh } from "./store/refresh"
import { createStoreState } from "./store/state-setup"

export function createBoardStore(api: TuiPluginApi, options?: Record<string, unknown>) {
  const squash = squashMerge(options)
  const setupCommands = commandPlan(options, "setup")
  const checkCommands = commandPlan(options, "check")
  const setup = setupCommands.map((command) => command.command).join(" && ") || undefined
  const check = checkCommands.map((command) => command.command).join(" && ") || undefined
  const scopes = configuredScopes(options)
  const sendBackThreshold = sendBackStopThreshold(options)
  const state = createStoreState(api, options)

  return {
    sessions: state.sessions,
    selected: state.selectedID,
    selectedSession: () => selectedSession(state.s),
    selectedColumn: state.selectedColumn,
    filter: state.filter,
    squashMerge: squash,
    setupCommand: setup,
    checkCommand: check,
    setupCommands,
    checkCommands,
    configuredScopes: scopes,
    sendBackStopThreshold: sendBackThreshold,
    inProgressCap: inProgressCap(options),
    columns: state.columns,
    notices: state.notices,
    notify: state.notify,
    updateStatus: state.updateStatus,
    setUpdateStatus: state.setUpdateStatus,
    select: (column: ColumnType, id: string | undefined) => select(state.s, column, id),
    selectNext: () => selectStep(state.s, 1),
    selectPrevious: () => selectStep(state.s, -1),
    selectNextColumn: () => selectColumnStep(state.s, 1),
    selectPrevColumn: () => selectColumnStep(state.s, -1),
    selectFirst: () => selectEdge(state.s, "first"),
    selectLast: () => selectEdge(state.s, "last"),
    reorder: (direction: 1 | -1) => reorder(state.s, direction),
    sessionStatus: (sessionID: string): SessionStatus["type"] | undefined => state.sessionStatuses()[sessionID],
    setSessionStatus: (sessionID: string, status: SessionStatus["type"]) =>
      state.setSessionStatuses((current) => ({ ...current, [sessionID]: status })),
    moveNext: () => moveByDirection(state.s, 1),
    movePrevious: () => moveByDirection(state.s, -1),
    moveTo: (status: ColumnType) => moveTo(state.s, status),
    moveDenyReason: (status: ColumnType, session: BoardSession) => moveDenyReason(state.s, status, session),
    deleteSelected: () => deleteSelected(state.s),
    refresh: () => refresh(state.s),
    setFilter(value: string) {
      state.setFilterSignal(value)
      persistFilter(api, value)
      select(state.s, state.selectedColumn(), state.selectedID())
    },
  }
}

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
