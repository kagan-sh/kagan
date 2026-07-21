import { helper, intakeReady, pendingFindingCount, sendBackStopThreshold } from "../domain/task/policy"
import { getStatus, kagan } from "../domain/task/metadata"
import type { HelperRole } from "../domain/task/types"

export function formatAge(updated: number, now: number): string {
  const diff = Math.max(0, now - updated)
  const minutes = Math.floor(diff / 60000)
  if (minutes < 1) return "<1m"
  if (minutes < 60) return `${minutes}m`
  const hours = Math.floor(minutes / 60)
  if (hours < 24) return `${hours}h`
  const days = Math.floor(hours / 24)
  return `${days}d`
}

export function confidenceBar(confidence?: number, width = 10): string {
  if (confidence === undefined) return "░".repeat(width)
  const filled = Math.round((Math.min(10, Math.max(0, confidence)) / 10) * width)
  return "█".repeat(filled) + "░".repeat(width - filled)
}

export function formatDiff(summary?: { additions: number; deletions: number; files: number }): string | undefined {
  if (!summary) return undefined
  const fileLabel = summary.files === 1 ? "file" : "files"
  return `+${summary.additions} -${summary.deletions} · ${summary.files} ${fileLabel}`
}

function stripSubagentSuffix(title: string): string {
  return title.replace(/\s*\(@[\w-]+ subagent\)$/i, "").trim()
}

export function shortSubtaskTitle(session: { title: string; slug: string }): string {
  return stripSubagentSuffix(session.title || session.slug)
}

export function summarizeSubtasks(children: readonly { title: string; slug: string }[], maxNames = 3): string {
  if (children.length === 0) return ""
  const names = children.slice(0, maxNames).map(shortSubtaskTitle)
  const extra = children.length - names.length
  const label = children.length === 1 ? "1 subtask" : `${children.length} subtasks`
  if (extra > 0) return `${label} · ${names.join(", ")}, +${extra}`
  return `${label} · ${names.join(", ")}`
}

function hasHelperError(metadata: Record<string, unknown> | undefined, role: HelperRole): boolean {
  return kagan(metadata).helperError?.role === role
}

export type Badge = { text: string; tone: "muted" | "success" | "warning" | "error" }

function commandBadge(
  label: "setup" | "check",
  result: { exitCode: number | null; steps?: { status: string }[] },
): Badge | undefined {
  const steps = result.steps
  if (!steps || steps.length === 0) {
    return result.exitCode === 0 ? { text: `${label} ok`, tone: "success" } : { text: `${label} failed`, tone: "error" }
  }
  const ran = steps.filter((step) => step.status === "ran").length
  const skipped = steps.length - ran
  if (ran === 0) return { text: `${label} skipped`, tone: "muted" }
  if (result.exitCode !== 0) return { text: `${label} failed`, tone: "error" }
  if (skipped > 0) return { text: `${label} partial`, tone: "success" }
  return { text: `${label} ok`, tone: "success" }
}

export function gateBadges(metadata?: Record<string, unknown>, threshold = sendBackStopThreshold()): Badge[] {
  const badges: Badge[] = []
  const view = kagan(metadata)
  const awaiting = view.awaitingPermissions?.length ?? 0
  if (awaiting > 0) badges.push({ text: awaiting > 1 ? `△ ${awaiting} need you` : "△ needs you", tone: "warning" })
  const intakeOutcome = helper(metadata, "intake").outcome
  if (getStatus(metadata) === "backlog" && view.boardTask === true && intakeOutcome !== undefined) {
    if (intakeOutcome === "failed" || hasHelperError(metadata, "intake")) {
      badges.push({ text: "intake failed", tone: "error" })
    } else {
      badges.push(intakeReady(metadata) ? { text: "intake ok", tone: "success" } : { text: "intake…", tone: "warning" })
    }
  }
  const mode = view.intake?.mode
  if (mode)
    badges.push({ text: `mode: ${mode.recommended === "autonomous" ? "auto" : mode.recommended}`, tone: "muted" })
  const setup = view.setup
  if (setup) {
    const badge = commandBadge("setup", setup)
    if (badge) badges.push(badge)
  }
  const check = view.check
  if (check) {
    const badge = commandBadge("check", check)
    if (badge) badges.push(badge)
  }
  const validatorOutcome = helper(metadata, "validator").outcome
  if (validatorOutcome === "pending") badges.push({ text: "reviewing…", tone: "muted" })
  else if (validatorOutcome === "failed" || hasHelperError(metadata, "validator")) {
    badges.push({ text: "review failed", tone: "error" })
  }
  if (validatorOutcome === "ran") {
    const pending = pendingFindingCount(metadata)
    badges.push(
      pending > 0 ? { text: `${pending} to triage`, tone: "warning" } : { text: "findings clear", tone: "success" },
    )
  }
  const generation = view.generation
  if (generation > 1) badges.push({ text: `iter ${generation}`, tone: generation >= threshold ? "warning" : "muted" })
  if (view.approved === true) badges.push({ text: "approved", tone: "success" })
  return badges
}

export function formatModeRationale(metadata?: Record<string, unknown>, checkCommand?: string): string | undefined {
  const mode = kagan(metadata).intake?.mode
  if (!mode) return undefined
  const overlay = checkCommand ? "" : " (no automatic check configured - lean assisted)"
  return `${mode.rationale}${overlay}`
}
