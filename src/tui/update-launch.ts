import type { TuiPluginApi, TuiPluginMeta } from "@opencode-ai/plugin/tui"
import { cleanupPreparedUpdate } from "./update-cleanup"
import { prepareUpdate } from "./update-manager"
import { checkForUpdate, type UpdateStatus } from "./updates"
import { defaultFileSystem, type FileSystem } from "./update-paths"

export function showUpdateToast(api: TuiPluginApi, currentVersion: string, status: Exclude<UpdateStatus, undefined>) {
  if (status.kind === "broken") return
  if (api.route.current.name !== "home" && api.route.current.name !== "session") return
  api.ui.toast({
    variant: status.kind === "ready" ? "success" : "warning",
    title: "Kagan",
    message:
      status.kind === "ready"
        ? `Kagan v${status.version} is ready. Restart OpenCode to apply.`
        : `Kagan v${currentVersion} remains active. Kagan v${status.version} requires OpenCode ${status.requiredOpenCode}.`,
  })
}

export async function runAutomaticUpdateLaunch(input: {
  api: TuiPluginApi
  meta: TuiPluginMeta
  currentVersion: string
  now: number
  setUpdateStatus: (status: UpdateStatus) => void
  showToast?: typeof showUpdateToast
  fetchImpl?: typeof fetch
  fs?: FileSystem
}): Promise<void> {
  const { api, meta, currentVersion, now, setUpdateStatus } = input
  const showToast = input.showToast ?? showUpdateToast
  const fs = input.fs ?? defaultFileSystem

  let cleanupFailed = false
  try {
    await cleanupPreparedUpdate(meta, currentVersion, fs)
  } catch {
    cleanupFailed = true
  }

  const status = await checkForUpdate({
    kv: api.kv,
    currentVersion,
    openCodeVersion: api.app.version,
    source: meta.source,
    spec: meta.spec,
    now,
    fetchImpl: input.fetchImpl,
  })

  if (cleanupFailed) {
    setUpdateStatus({ kind: "broken" })
    return
  }

  if (!status || api.lifecycle.signal.aborted) return

  if (status.kind === "ready") {
    await prepareUpdate({ api, meta, currentVersion, status, fs })
  }

  setUpdateStatus(status)
  showToast(api, currentVersion, status)
}
