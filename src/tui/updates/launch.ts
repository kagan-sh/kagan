import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { checkForUpdate, type UpdateStatus } from "./check"

export async function runUpdateDiscovery(input: {
  api: TuiPluginApi
  meta: TuiPluginMeta
  currentVersion: string
  now: number
  setUpdateStatus: (status: UpdateStatus) => void
  fetchImpl?: typeof fetch
}): Promise<void> {
  const result = await checkForUpdate({
    kv: input.api.kv,
    currentVersion: input.currentVersion,
    source: input.meta.source,
    spec: input.meta.spec,
    now: input.now,
    fetchImpl: input.fetchImpl,
  })
  if (result?.kind === "available" && !input.api.lifecycle.signal.aborted) input.setUpdateStatus(result)
}
