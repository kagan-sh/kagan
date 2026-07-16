import type { TuiPluginApi, TuiPluginMeta, TuiToast } from "@opencode-ai/plugin/tui"
import { version as currentVersion } from "../../../package.json"
import { ROUTE } from "../types"
import { checkForUpdate, type UpdateCheck, type UpdateStatus } from "./check"
import { confirmUpdate } from "./confirm"
import { installUpdate } from "./install"
import { handleUpdateCheckResult } from "./run-check"
import { runGlobalPluginUpdate, type UpdateCommandResult } from "./runner"

type UpdateStore = {
  setUpdateStatus: (status: UpdateStatus) => void
  notify: (toast: TuiToast) => void
}

type ConfirmUpdate = (current: string, target: string) => Promise<boolean>

export function createUpdateController(input: {
  api: TuiPluginApi
  meta: TuiPluginMeta
  store: UpdateStore
  now?: () => number
  check?: (force: boolean) => Promise<UpdateCheck>
  confirm?: ConfirmUpdate
  runCommand?: (version: string, cwd: string) => Promise<UpdateCommandResult>
}) {
  const { api, meta, store } = input
  let active: Promise<void> | undefined
  const notify = (toast: TuiToast) => {
    if (api.route.current.name === ROUTE) store.notify(toast)
    else api.ui.toast(toast)
  }
  const check =
    input.check ??
    ((force) =>
      checkForUpdate({
        kv: api.kv,
        currentVersion,
        source: meta.source,
        spec: meta.spec,
        now: (input.now ?? Date.now)(),
        force,
      }))
  const confirm: ConfirmUpdate = input.confirm ?? ((current, target) => confirmUpdate(api, current, target))
  const runCommand = input.runCommand ?? runGlobalPluginUpdate

  // Guard spans check → confirm → install so a second update request during the open dialog can't
  // race a concurrent global install.
  const run = () => {
    if (active) return active
    active = (async () => {
      const result = await check(true)
      if (handleUpdateCheckResult({ store, notify, result, currentVersion }) === "stop") return
      const target = result?.kind === "available" ? result.version : undefined
      if (!target) return
      if (await confirm(currentVersion, target))
        await installUpdate({ api, store, version: target, notify, runCommand })
    })().finally(() => {
      active = undefined
    })
    return active
  }

  return { run }
}
