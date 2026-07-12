import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import type { ColumnType } from "../../../domain/task/types"
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
    awaitingInputSeen: Set<string>
  }
}
