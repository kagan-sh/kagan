import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { sortFindingsByConfidence, type Finding, type FindingResolution } from "../../../domain/task/findings"
import { isSubstantive } from "../../../domain/task/intake"
import { kagan } from "../../../domain/task/metadata"
import { resolveSessionFinding } from "../../session/tasks"
import type { BoardStore } from "../../board/store"
import type { BoardSession } from "../../types"
import type { FindingsMode } from "./views"

const SLOT_RESOLUTIONS = [undefined, "ignored", "intended", "clarified"] as const satisfies readonly (
  | FindingResolution
  | undefined
)[]

function requiresNote(resolution: FindingResolution, finding: Finding): boolean {
  if (resolution === "ignored" || resolution === "clarified") return true
  return resolution === "intended" && finding.severity === "high"
}

export async function commitFindingResolution(props: {
  api: TuiPluginApi
  store: BoardStore
  sessionId: string
  session: BoardSession
  finding: Finding
  resolution: FindingResolution
  note: string
  setSession: (value: BoardSession | ((prev: BoardSession) => BoardSession)) => BoardSession
  setMode: (value: FindingsMode | ((prev: FindingsMode) => FindingsMode)) => FindingsMode
  setError: (value: string | undefined | ((prev: string | undefined) => string | undefined)) => string | undefined
}): Promise<void> {
  const noteValue = props.note.trim()
  if (requiresNote(props.resolution, props.finding) && !isSubstantive(noteValue)) {
    props.setError("Add a substantive note to rule this way")
    return
  }
  props.setError(undefined)
  try {
    await resolveSessionFinding(
      props.api,
      props.sessionId,
      props.session,
      props.finding.id,
      props.resolution,
      noteValue || undefined,
    )
    await props.store.refresh()
    const refreshed = props.store.sessions().find((item) => item.id === props.sessionId)
    if (refreshed) props.setSession(refreshed as BoardSession)
    props.setMode("list")
  } catch (err) {
    props.setError(err instanceof Error ? err.message : String(err))
  }
}

export function handleFindingsListKey(props: {
  key: { name: string }
  findings: Finding[]
  index: () => number
  setIndex: (value: number | ((prev: number) => number)) => number
  close: () => void
  runSendBack: () => void
  runApprove: () => void
  openDetail: (index: number) => void
}): boolean {
  if (props.key.name === "escape") {
    props.close()
    return true
  }
  if (props.key.name === "s") {
    props.runSendBack()
    return true
  }
  if (props.key.name === "a") {
    props.runApprove()
    return true
  }
  if (props.findings.length === 0) return true
  if (props.key.name === "down" || props.key.name === "j") {
    props.setIndex((i) => Math.min(i + 1, props.findings.length - 1))
    return true
  }
  if (props.key.name === "up" || props.key.name === "k") {
    props.setIndex((i) => Math.max(i - 1, 0))
    return true
  }
  if (props.key.name === "return") {
    props.openDetail(props.index())
    return true
  }
  return false
}

export function handleFindingsDetailKey(props: {
  key: { name: string; shift?: boolean }
  focus: () => number
  setFocus: (value: number | ((prev: number) => number)) => number
  setError: (value: string | undefined | ((prev: string | undefined) => string | undefined)) => string | undefined
  setMode: (value: FindingsMode | ((prev: FindingsMode) => FindingsMode)) => FindingsMode
  commit: (resolution: FindingResolution) => void
}): boolean {
  if (props.key.name === "escape") {
    props.setError(undefined)
    props.setMode("list")
    return true
  }
  const slots = SLOT_RESOLUTIONS.length
  if (props.key.name === "tab") {
    props.setFocus((f) => (props.key.shift ? (f + slots - 1) % slots : (f + 1) % slots))
    return true
  }
  if (props.key.name === "down") {
    props.setFocus((f) => (f + 1) % slots)
    return true
  }
  if (props.key.name === "up") {
    props.setFocus((f) => (f + slots - 1) % slots)
    return true
  }
  if (props.key.name === "return" && props.focus() > 0) {
    const resolution = SLOT_RESOLUTIONS[props.focus()]
    if (resolution) props.commit(resolution)
    return true
  }
  return false
}

export function sortedFindings(metadata: Record<string, unknown>): Finding[] {
  return sortFindingsByConfidence(kagan(metadata).findings ?? [])
}
