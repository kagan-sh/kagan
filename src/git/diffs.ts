import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import type { GitRunner } from "./runner"

function parseNumstat(text: string): Map<string, { additions: number; deletions: number }> {
  const result = new Map<string, { additions: number; deletions: number }>()
  for (const line of text.split("\n")) {
    const match = line.match(/^(\d+|-)\t(\d+|-)\t(.+)$/)
    if (!match) continue
    const [, additions, deletions, file] = match
    if (!additions || !deletions || !file) continue
    result.set(file, {
      additions: additions === "-" ? 0 : Number(additions),
      deletions: deletions === "-" ? 0 : Number(deletions),
    })
  }
  return result
}

function parseNameStatus(text: string): Map<string, SnapshotFileDiff["status"]> {
  const statusByCode: Record<string, SnapshotFileDiff["status"]> = { A: "added", D: "deleted", M: "modified" }
  const result = new Map<string, SnapshotFileDiff["status"]>()
  for (const line of text.split("\n")) {
    const match = line.match(/^([A-Z])\t(.+)$/)
    if (!match) continue
    const [, code, file] = match
    if (!code || !file) continue
    result.set(file, statusByCode[code] ?? "modified")
  }
  return result
}

function splitPatchByFile(text: string): Map<string, string> {
  const result = new Map<string, string>()
  const sections = text.split(/^(?=diff --git )/m).filter(Boolean)
  for (const section of sections) {
    const header = section.match(/^diff --git a\/.+? b\/(.+)$/m)
    if (!header) continue
    const [, file] = header
    if (file) result.set(file, section)
  }
  return result
}

const DEPENDENCY_MANIFESTS = new Set([
  "bun.lock",
  "bun.lockb",
  "package.json",
  "package-lock.json",
  "yarn.lock",
  "pnpm-lock.yaml",
  "npm-shrinkwrap.json",
  "Cargo.toml",
  "Cargo.lock",
  "pyproject.toml",
  "poetry.lock",
  "uv.lock",
  "Pipfile",
  "Pipfile.lock",
  "requirements.txt",
  "go.mod",
  "go.sum",
  "Gemfile",
  "Gemfile.lock",
  "composer.json",
  "composer.lock",
])

function riskScore(diff: SnapshotFileDiff): number {
  const file = diff.file ?? ""
  const basename = file.split("/").pop() ?? ""
  const boost = DEPENDENCY_MANIFESTS.has(basename)
    ? 15000
    : /[._-](test|spec)\./i.test(basename) || /\/(tests?|specs?|__tests__)\//i.test(file)
      ? 10000
      : /config|rc$|\.json$|\.ya?ml$|\.toml$|\.ini$|\.conf$|\.env$/.test(basename.toLowerCase())
        ? 5000
        : 0
  return (diff.additions ?? 0) + (diff.deletions ?? 0) + boost
}

export function orderDiffsByRisk(diffs: Array<SnapshotFileDiff>): Array<SnapshotFileDiff> {
  return diffs.slice().sort((left, right) => {
    const scoreDiff = riskScore(right) - riskScore(left)
    if (scoreDiff !== 0) return scoreDiff
    return (left.file ?? "").localeCompare(right.file ?? "")
  })
}

async function diffBase(run: GitRunner, worktree: string, baseBranch: string): Promise<string | undefined> {
  const mergeBase = await run(["merge-base", baseBranch, "HEAD"], worktree)
  if (mergeBase.code === 0 && mergeBase.stdout.trim()) return mergeBase.stdout.trim()
  const ref = await run(["rev-parse", "--verify", baseBranch], worktree)
  if (ref.code === 0 && ref.stdout.trim()) return ref.stdout.trim()
  return undefined
}

async function untrackedDiffs(run: GitRunner, worktree: string): Promise<SnapshotFileDiff[]> {
  const listed = await run(["ls-files", "--others", "--exclude-standard"], worktree)
  if (listed.code !== 0) return []
  const files = listed.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
  const diffs: SnapshotFileDiff[] = []
  for (const file of files) {
    const patch = await run(
      ["-c", "core.quotePath=false", "diff", "--no-color", "--no-index", "--", "/dev/null", file],
      worktree,
    )
    const additions = patch.stdout.split("\n").filter((line) => line.startsWith("+") && !line.startsWith("+++")).length
    diffs.push({ file, patch: patch.stdout, additions, deletions: 0, status: "added" })
  }
  return diffs
}

export async function worktreeDiffs(run: GitRunner, worktree: string, baseBranch: string): Promise<SnapshotFileDiff[]> {
  const base = await diffBase(run, worktree, baseBranch)
  if (!base) return []
  const gitDiff = (mode: string) =>
    run(["-c", "core.quotePath=false", "diff", "--no-renames", "--no-color", mode, base], worktree)
  const [numstat, nameStatus, patch] = await Promise.all([
    gitDiff("--numstat"),
    gitDiff("--name-status"),
    gitDiff("--patch"),
  ])
  if (numstat.code !== 0) return []
  const counts = parseNumstat(numstat.stdout)
  const statuses = parseNameStatus(nameStatus.stdout)
  const patches = splitPatchByFile(patch.stdout)
  const tracked: SnapshotFileDiff[] = [...counts.entries()].map(([file, count]) => ({
    file,
    additions: count.additions,
    deletions: count.deletions,
    status: statuses.get(file) ?? "modified",
    patch: patches.get(file) ?? "",
  }))
  return [...tracked, ...(await untrackedDiffs(run, worktree))].sort((left, right) =>
    (left.file ?? "").localeCompare(right.file ?? ""),
  )
}
