import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { isSubstantive } from "./intake"

type HunkRange = { start: number; end: number }

export function newSideHunkRanges(patch: string): HunkRange[] {
  const ranges: HunkRange[] = []
  for (const match of patch.matchAll(/^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/gm)) {
    const start = Number(match[1])
    ranges.push({ start, end: start + (match[2] === undefined ? 1 : Number(match[2])) })
  }
  return ranges
}

type FindingCategory = "misalignment" | "bug" | "uncertainty"
export type FindingResolution = "ignored" | "intended" | "clarified"

export type Finding = {
  id: string
  summary: string
  detail?: string
  location?: string
  severity?: "low" | "medium" | "high"
  confidence?: number
  category?: FindingCategory
  resolution?: FindingResolution
  note?: string
  outOfDiff?: true
}

const MAX_UNVERIFIED_CONFIDENCE = 2

function locationParts(location: string): { file: string; line?: number } {
  const separator = location.lastIndexOf(":")
  if (separator === -1) return { file: location }
  const linePart = location.slice(separator + 1)
  if (!/^\d+$/.test(linePart)) return { file: location }
  return { file: location.slice(0, separator), line: Number(linePart) }
}

function citationInDiff(location: string, diffs: readonly SnapshotFileDiff[]): boolean {
  const { file, line } = locationParts(location)
  const diff = diffs.find((candidate) => candidate.file === file)
  if (!diff) return false
  if (line === undefined) return true
  return newSideHunkRanges(diff.patch ?? "").some((range) => line >= range.start && line < range.end)
}

export function verifyFindingCitations(findings: readonly Finding[], diffs: readonly SnapshotFileDiff[]): Finding[] {
  return findings.map((finding) => {
    if (!finding.location || citationInDiff(finding.location, diffs)) return finding
    return {
      ...finding,
      confidence: Math.min(finding.confidence ?? MAX_UNVERIFIED_CONFIDENCE, MAX_UNVERIFIED_CONFIDENCE),
      outOfDiff: true,
    }
  })
}

export function isResolvedFinding(finding: Finding): boolean {
  if (finding.resolution === "ignored" || finding.resolution === "clarified") {
    return typeof finding.note === "string" && isSubstantive(finding.note)
  }
  if (finding.resolution === "intended") {
    return finding.severity !== "high" || (typeof finding.note === "string" && isSubstantive(finding.note))
  }
  return false
}

const CONFIDENCE_UNSCORED = -1

export function sortFindingsByConfidence(findings: readonly Finding[]): Finding[] {
  return [...findings].sort(
    (left, right) => (right.confidence ?? CONFIDENCE_UNSCORED) - (left.confidence ?? CONFIDENCE_UNSCORED),
  )
}

export function resolveFinding(
  findings: readonly Finding[],
  findingID: string,
  resolution: FindingResolution,
  note?: string,
): Finding[] {
  return findings.map((finding) => {
    if (finding.id !== findingID) return finding
    return { ...finding, resolution, ...(note === undefined ? {} : { note }) }
  })
}
