import type { TuiPluginApi, TuiPluginMeta, TuiToast } from "@opencode-ai/plugin/tui"
import { version as currentVersion } from "../../../package.json"
import { ROUTE } from "../types"
import { checkForUpdate, KAGAN_PACKAGE, type UpdateCheck, type UpdateStatus } from "./check"
import { runGlobalPluginUpdate, type UpdateCommandResult } from "./runner"

type UpdateStore = {
  setUpdateStatus: (status: UpdateStatus) => void
  notify: (toast: TuiToast) => void
}

type ConfirmUpdate = (current: string, target: string, onConfirm: () => void) => void

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
  const confirm: ConfirmUpdate =
    input.confirm ??
    ((current, target, onConfirm) => {
      api.ui.dialog.replace(() =>
        api.ui.DialogConfirm({
          title: "Update Kagan",
          message: `Update Kagan from v${current} to v${target}? Restart OpenCode after installation.`,
          onConfirm: () => {
            api.ui.dialog.clear()
            onConfirm()
          },
          onCancel: () => api.ui.dialog.clear(),
        }),
      )
    })
  const runCommand = input.runCommand ?? runGlobalPluginUpdate

  const fail = (version: string, message: string) => {
    store.setUpdateStatus({ kind: "available", version })
    notify({ variant: "error", title: "Kagan update failed", message })
  }

  const install = (version: string) => {
    if (active) return active
    active = (async () => {
      store.setUpdateStatus({ kind: "installing", version })
      const exact = `${KAGAN_PACKAGE}@${version}`
      const staged = await api.plugins.add(exact).catch(() => false)
      if (!staged) {
        fail(version, `OpenCode could not stage ${exact}.`)
        return
      }
      const result = await runCommand(version, api.state.path.directory)
      if (!result.ok) {
        const detail =
          result.output || (result.exitCode === null ? "Unable to run OpenCode." : `Exited ${result.exitCode}.`)
        fail(version, detail)
        return
      }
      store.setUpdateStatus({ kind: "restart", version })
      notify({
        variant: "success",
        title: "Kagan updated",
        message: `Kagan v${version} is installed. Restart OpenCode.`,
      })
    })().finally(() => {
      active = undefined
    })
    return active
  }

  const run = () => {
    if (active) return active
    active = (async () => {
      const result = await check(true)
      if (!result) {
        notify({ variant: "warning", title: "Kagan", message: "Unable to check for updates." })
        return
      }
      if (result.kind === "current") {
        store.setUpdateStatus(undefined)
        notify({ variant: "info", title: "Kagan", message: `Kagan v${currentVersion} is current.` })
        return
      }
      store.setUpdateStatus(result)
      confirm(currentVersion, result.version, () => void install(result.version))
    })().finally(() => {
      active = undefined
    })
    return active
  }

  return { run }
}
