import type { UpdateStatus } from "../../updates/check"

export function updateFooter(status: Exclude<UpdateStatus, undefined>) {
  if (status.kind === "available") return ` · v${status.version} available`
  if (status.kind === "installing") return ` · installing v${status.version}`
  return ` · v${status.version} installed - restart OpenCode`
}
