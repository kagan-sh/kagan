import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createMemo, createSignal } from "solid-js"
import type { SessionStatus } from "@opencode-ai/sdk/v2"
import { getFilter } from "../../session/preferences"
import type { ColumnType } from "../../../domain/task/types"
import type { BoardSession } from "../../types"
import type { UpdateStatus } from "../../updates/check"
import { filterSessions, groupCardsByColumn } from "./sessions"
import { createNotices } from "./notices"
import type { StoreState } from "./selection"

export function createStoreState(api: TuiPluginApi, options?: Record<string, unknown>) {
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
    s,
    sessions,
    selectedID,
    selectedColumn,
    filter,
    columns,
    notices,
    notify,
    updateStatus,
    setUpdateStatus,
    sessionStatuses,
    setSessionStatuses,
    setFilterSignal,
  }
}
