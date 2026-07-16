import type { TuiToast } from "@opencode-ai/plugin/tui"
import type { UpdateCheck, UpdateStatus } from "./check"

type UpdateStore = {
  setUpdateStatus: (status: UpdateStatus | undefined) => void
}

export function handleUpdateCheckResult(input: {
  store: UpdateStore
  notify: (toast: TuiToast) => void
  result: UpdateCheck | undefined
  currentVersion: string
}): "stop" | "continue" {
  if (input.result?.kind === "ineligible") {
    input.notify({
      variant: "info",
      title: "Kagan",
      message: "Updates apply only to global npm installs.",
    })
    return "stop"
  }
  if (!input.result) {
    input.notify({ variant: "warning", title: "Kagan", message: "Unable to check for updates." })
    return "stop"
  }
  if (input.result.kind === "current") {
    input.store.setUpdateStatus(undefined)
    input.notify({ variant: "info", title: "Kagan", message: `Kagan v${input.currentVersion} is current.` })
    return "stop"
  }
  input.store.setUpdateStatus(input.result)
  return "continue"
}
