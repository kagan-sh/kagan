import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import { KAGAN_PACKAGE, type UpdateStatus } from "./check"
import type { UpdateCommandResult } from "./runner"

type UpdateStore = {
  setUpdateStatus: (status: UpdateStatus) => void
}

export async function installUpdate(input: {
  api: TuiPluginApi
  store: UpdateStore
  version: string
  notify: (toast: TuiToast) => void
  runCommand: (version: string, cwd: string) => Promise<UpdateCommandResult>
}): Promise<void> {
  const fail = (message: string) => {
    input.store.setUpdateStatus({ kind: "available", version: input.version })
    input.notify({ variant: "error", title: "Kagan update failed", message })
  }

  input.store.setUpdateStatus({ kind: "installing", version: input.version })
  const exact = `${KAGAN_PACKAGE}@${input.version}`
  const staged = await input.api.plugins.add(exact).catch(() => false)
  if (!staged) {
    fail(`OpenCode could not stage ${exact}.`)
    return
  }
  const result = await input.runCommand(input.version, input.api.state.path.directory)
  if (!result.ok) {
    const detail =
      result.output || (result.exitCode === null ? "Unable to run OpenCode." : `Exited ${result.exitCode}.`)
    fail(detail)
    return
  }
  input.store.setUpdateStatus({ kind: "restart", version: input.version })
  input.notify({
    variant: "success",
    title: "Kagan updated",
    message: `Kagan v${input.version} is installed. Restart OpenCode.`,
  })
}
