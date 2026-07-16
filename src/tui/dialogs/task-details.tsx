import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { getStatus, kagan } from "../../domain/task/metadata"
import type { Finding } from "../../domain/task/findings"
import type { Intake } from "../../domain/task/intake"
import type { ColumnType } from "../../domain/task/types"
import type { CheckResult } from "../../checks/runner"

export type TaskDetails = {
  title?: string
  status?: ColumnType
  taskNumber?: number
  report?: string
  description?: string
  baseBranch?: string
  generation: number
  approved: boolean
  findings: Finding[]
  priorTriage: Finding[]
  intake?: Intake
  check?: CheckResult
  setup?: CheckResult
  diffStats: Array<{ file: string; additions: number; deletions: number; status?: string }>
}

export function buildTaskDetails(
  metadata: Record<string, unknown>,
  diffs: Array<SnapshotFileDiff>,
  title?: string,
): TaskDetails {
  const view = kagan(metadata)
  return {
    title,
    status: getStatus(metadata),
    taskNumber: view.taskNumber,
    report: view.report,
    description: view.description,
    baseBranch: view.baseBranch,
    generation: view.generation,
    approved: view.approved === true,
    findings: view.findings ?? [],
    priorTriage: view.priorTriage ?? [],
    intake: view.intake,
    check: view.check,
    setup: view.setup,
    diffStats: diffs.map((diff) => ({
      file: diff.file ?? "unknown",
      additions: diff.additions ?? 0,
      deletions: diff.deletions ?? 0,
      status: diff.status,
    })),
  }
}
