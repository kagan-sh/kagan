import type { SessionStatus } from "@opencode-ai/sdk/v2"
import { intakeReady } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import type { Badge } from "../../format"
import { formatAge, formatDiff, gateBadges } from "../../format"
import type { BoardSession } from "../../types"

export function isReady(session: BoardSession): boolean {
  return (
    session.kaganStatus === "backlog" && kagan(session.metadata).boardTask === true && intakeReady(session.metadata)
  )
}

export function taskLabel(session: BoardSession): string {
  const title = session.title || session.slug
  const number = kagan(session.metadata).taskNumber
  return number !== undefined ? `#${number} ${title}` : title
}

export function detailSegments(
  session: BoardSession,
  selected: boolean,
  now: number,
  sendBackStopThreshold?: number,
): Badge[] {
  const segments: Badge[] = [{ text: formatAge(session.time.updated, now), tone: "muted" }]
  if (selected) {
    const diff = formatDiff(session.summary)
    if (diff) segments.push({ text: diff, tone: "muted" })
  }
  segments.push(...gateBadges(session.metadata, sendBackStopThreshold))
  return segments
}

export function workingBadge(
  status: SessionStatus["type"] | undefined,
): { text: string; tone: "success" | "warning" } | undefined {
  if (status === "busy") return { text: " · ● working", tone: "success" }
  if (status === "retry") return { text: " · ↻ retrying", tone: "warning" }
  return undefined
}
