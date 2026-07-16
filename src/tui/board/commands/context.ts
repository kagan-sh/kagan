import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { createBoardStore } from "../store"

export type BoardStore = ReturnType<typeof createBoardStore>

export type BoardActions = {
  api: TuiPluginApi
  store: BoardStore
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void
  notifyError: (message: string) => void
  notifyWarning: (message: string) => void
  notifyErrorFrom: (error: unknown) => void
}

export function createBoardActions(
  api: TuiPluginApi,
  store: BoardStore,
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void,
): BoardActions {
  return {
    api,
    store,
    setHelpOpen,
    notifyError: (message) => store.notify({ variant: "error", title: "Kagan", message }),
    notifyWarning: (message) => store.notify({ variant: "warning", title: "Kagan", message }),
    notifyErrorFrom: (error) =>
      store.notify({
        variant: "error",
        title: "Kagan",
        message: error instanceof Error ? error.message : String(error),
      }),
  }
}
