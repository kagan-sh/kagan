import type { UpdateStatus } from "../../updates/check"

export function updateFooter(status: Exclude<UpdateStatus, undefined>) {
  if (status.kind === "ready") return ` · v${status.version} ready — restart OpenCode`
  if (status.kind === "blocked") {
    return ` · update OpenCode to ${status.requiredOpenCode} for Kagan v${status.version}`
  }
  return " · updates unavailable"
}
