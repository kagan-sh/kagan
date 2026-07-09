import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { orderDiffsByRisk } from "../../git/diffs"

const PATCH_CHAR_LIMIT = 8000
const DIFF_CHAR_BUDGET = 60000
const LOCKFILE_BASENAMES = new Set([
  "bun.lock",
  "bun.lockb",
  "package-lock.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "Cargo.lock",
  "poetry.lock",
  "uv.lock",
])

function statLine(file: string, reason: string, diff: SnapshotFileDiff): string {
  return `--- ${file} (${reason}, +${diff.additions ?? 0}/-${diff.deletions ?? 0})`
}

function truncatePatch(patch: string): string {
  if (patch.length <= PATCH_CHAR_LIMIT) return patch
  return `${patch.slice(0, PATCH_CHAR_LIMIT)}\n[patch truncated — showing ${PATCH_CHAR_LIMIT} of ${patch.length} chars; read the file in the worktree for full context]`
}

export function formatDiffsForPrompt(diffs: Array<SnapshotFileDiff>): string {
  let budgetRemaining = DIFF_CHAR_BUDGET
  let budgetExhausted = false
  return orderDiffsByRisk(diffs)
    .map((diff) => {
      const file = diff.file ?? "unknown"
      const basename = file.split("/").pop() ?? file
      if (LOCKFILE_BASENAMES.has(basename)) return statLine(file, "lockfile — patch omitted", diff)

      const patch = truncatePatch(diff.patch ?? "")
      if (budgetExhausted || patch.length > budgetRemaining) {
        budgetExhausted = true
        return statLine(file, "patch omitted — diff budget exhausted", diff)
      }
      budgetRemaining -= patch.length
      return `--- ${file}\n${patch}`
    })
    .join("\n\n")
}
