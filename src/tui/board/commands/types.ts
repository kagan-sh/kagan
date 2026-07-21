import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { BoardStore } from "../store"

export type BoardCommandContext = {
  api: TuiPluginApi
  store: BoardStore
  setHelpOpen: (value: boolean | ((prev: boolean) => boolean)) => void
}

export async function selectSessionOrNotify(ctx: BoardCommandContext, sessionID: string) {
  try {
    await ctx.api.client.tui.selectSession({ sessionID }, { throwOnError: true })
  } catch (error) {
    ctx.store.notify({
      variant: "error",
      title: "Kagan",
      message: error instanceof Error ? error.message : String(error),
    })
  }
}
