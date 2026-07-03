import type { PluginInput } from "@opencode-ai/plugin"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { mkdir, readFile, rm, writeFile } from "node:fs/promises"
import { homedir, tmpdir } from "node:os"
import { join, resolve } from "node:path"

export type GitResult = { code: number; stdout: string; stderr: string }
export type GitRunner = (args: string[], cwd: string) => Promise<GitResult>

export function bunGitRunner(): GitRunner {
  return async (args, cwd) => {
    const proc = Bun.spawn(["git", ...args], { cwd, stdout: "pipe", stderr: "pipe", stdin: "ignore" })
    const [stdout, stderr, code] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ])
    return { code, stdout, stderr }
  }
}

export function shellGitRunner($: PluginInput["$"]): GitRunner {
  return async (args, cwd) => {
    const result = await $`git -C ${cwd} ${args}`.nothrow().quiet()
    return { code: result.exitCode, stdout: result.stdout.toString(), stderr: result.stderr.toString() }
  }
}

export function slugifyTitle(title: string): string {
  const base = title
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 40)
    .replace(/-+$/, "")
  return base || "task"
}

export function uniqueTaskSlug(title: string): string {
  return `${slugifyTitle(title)}-${Math.random().toString(36).slice(2, 6)}`
}

export function taskBranch(slug: string): string {
  return `kagan/${slug}`
}

const COMMAND_SEPARATORS = /&&|\|\||[;\n|]/

// Naive whitespace tokenization (no quote-awareness), matching from the first `git` token wherever
// it appears: over-flagging prose that mentions a push (e.g. `echo "git push"`) is harmless, while
// missing a real push behind an env-var or sudo prefix is not.
function isGitPushSegment(segment: string): boolean {
  const tokens = segment.trim().split(/\s+/)
  const start = tokens.indexOf("git")
  if (start === -1) return false
  for (let i = start + 1; i < tokens.length; i++) {
    const token = tokens[i]!
    if (token === "push") return true
    if (token === "-C" || token === "-c") {
      i++
      continue
    }
    if (token.startsWith("-")) continue
    return false
  }
  return false
}

export function isGitPushCommand(command: string): boolean {
  return command.split(COMMAND_SEPARATORS).some(isGitPushSegment)
}

export function taskWorktreePath(mainWorktree: string, slug: string): string {
  return join(homedir(), ".kagan", "worktrees", Bun.hash(mainWorktree).toString(16), slug)
}

export async function baseBranchFreshness(
  run: GitRunner,
  worktree: string | undefined,
  baseBranch: string | undefined,
): Promise<{ ahead: number }> {
  if (!worktree || !baseBranch) return { ahead: 0 }
  const result = await run(["rev-list", "--count", `HEAD..${baseBranch}`], worktree)
  if (result.code !== 0) return { ahead: 0 }
  const ahead = Number(result.stdout.trim())
  return Number.isNaN(ahead) ? { ahead: 0 } : { ahead }
}

export async function listLocalBranches(run: GitRunner, repoDir: string): Promise<string[]> {
  const result = await run(
    ["for-each-ref", "refs/heads", "--format=%(refname:short)", "--sort=-committerdate"],
    repoDir,
  )
  if (result.code !== 0) return []
  return result.stdout
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
}

export async function currentBranch(run: GitRunner, repoDir: string): Promise<string | undefined> {
  const result = await run(["branch", "--show-current"], repoDir)
  const branch = result.stdout.trim()
  return result.code === 0 && branch ? branch : undefined
}

export async function createTaskWorktree(
  run: GitRunner,
  mainWorktree: string,
  slug: string,
  baseBranch: string,
): Promise<{ directory: string; branch: string }> {
  const directory = taskWorktreePath(mainWorktree, slug)
  const branch = taskBranch(slug)
  await mkdir(join(directory, ".."), { recursive: true })
  const result = await run(["worktree", "add", "-b", branch, directory, baseBranch], mainWorktree)
  if (result.code !== 0) {
    throw new Error(result.stderr.trim() || result.stdout.trim() || "git worktree add failed")
  }
  return { directory, branch }
}

const kaganPackageRoot = resolve(import.meta.dir, "..")

// OpenCode boots a separate instance per session directory and delivers events only to
// plugins that instance loaded itself (the host drops events whose directory differs from
// the plugin's). Task worktrees are fresh checkouts that carry no kagan registration, so
// without this file no plugin instance ever sees the task session's events and
// intake/review never start.
export async function ensureWorktreePluginConfig(directory: string, pluginSpec: string = kaganPackageRoot) {
  const configDir = join(directory, ".opencode")
  const configFile = join(configDir, "opencode.json")
  const existing = await readFile(configFile, "utf8").catch(() => undefined)
  let config: Record<string, unknown> = { $schema: "https://opencode.ai/config.json" }
  if (existing !== undefined) {
    try {
      config = JSON.parse(existing) as Record<string, unknown>
    } catch (error) {
      throw new Error(`Cannot register kagan plugin: ${configFile} is not valid JSON (${error})`)
    }
  }
  const plugins = Array.isArray(config.plugin) ? (config.plugin as unknown[]) : []
  if (plugins.includes(pluginSpec)) return
  config.plugin = [...plugins, pluginSpec]
  await mkdir(configDir, { recursive: true })
  await writeFile(configFile, `${JSON.stringify(config, null, 2)}\n`)
}

export function parseNumstat(text: string): Map<string, { additions: number; deletions: number }> {
  const result = new Map<string, { additions: number; deletions: number }>()
  for (const line of text.split("\n")) {
    const match = line.match(/^(\d+|-)\t(\d+|-)\t(.+)$/)
    if (!match) continue
    result.set(match[3]!, {
      additions: match[1] === "-" ? 0 : Number(match[1]),
      deletions: match[2] === "-" ? 0 : Number(match[2]),
    })
  }
  return result
}

export function parseNameStatus(text: string): Map<string, SnapshotFileDiff["status"]> {
  const statusByCode: Record<string, SnapshotFileDiff["status"]> = { A: "added", D: "deleted", M: "modified" }
  const result = new Map<string, SnapshotFileDiff["status"]>()
  for (const line of text.split("\n")) {
    const match = line.match(/^([A-Z])\t(.+)$/)
    if (!match) continue
    result.set(match[2]!, statusByCode[match[1]!] ?? "modified")
  }
  return result
}

export function splitPatchByFile(text: string): Map<string, string> {
  const result = new Map<string, string>()
  const sections = text.split(/^(?=diff --git )/m).filter(Boolean)
  for (const section of sections) {
    const header = section.match(/^diff --git a\/.+? b\/(.+)$/m)
    if (!header) continue
    result.set(header[1]!, section)
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

function isDependencyManifest(file: string): boolean {
  const basename = file.split("/").pop() ?? ""
  return DEPENDENCY_MANIFESTS.has(basename)
}

function isTestOrSpec(file: string): boolean {
  const basename = file.split("/").pop() ?? ""
  return /[._-](test|spec)\./i.test(basename) || /\/(tests?|specs?|__tests__)\//i.test(file)
}

function isConfigFile(file: string): boolean {
  const lower = (file.split("/").pop() ?? "").toLowerCase()
  return /config|rc$|\.json$|\.ya?ml$|\.toml$|\.ini$|\.conf$|\.env$/.test(lower)
}

function riskScore(diff: SnapshotFileDiff): number {
  const file = diff.file ?? ""
  let boost = 0
  if (isDependencyManifest(file)) {
    boost = 15000
  } else if (isTestOrSpec(file)) {
    boost = 10000
  } else if (isConfigFile(file)) {
    boost = 5000
  }
  return (diff.additions ?? 0) + (diff.deletions ?? 0) + boost
}

export function orderDiffsByRisk(diffs: Array<SnapshotFileDiff>): Array<SnapshotFileDiff> {
  return diffs.slice().sort((left, right) => {
    const scoreDiff = riskScore(right) - riskScore(left)
    if (scoreDiff !== 0) return scoreDiff
    return (left.file ?? "").localeCompare(right.file ?? "")
  })
}

export type HunkRange = { start: number; end: number }

export function newSideHunkRanges(patch: string): HunkRange[] {
  const ranges: HunkRange[] = []
  const hunkHeader = /^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@/gm
  let match: RegExpExecArray | null
  while ((match = hunkHeader.exec(patch)) !== null) {
    const start = Number(match[1])
    const length = match[2] === undefined ? 1 : Number(match[2])
    ranges.push({ start, end: start + length })
  }
  return ranges
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

export async function commitAll(run: GitRunner, worktree: string, message: string): Promise<GitResult | undefined> {
  const status = await run(["status", "--porcelain"], worktree)
  if (status.code !== 0 || !status.stdout.trim()) return undefined
  const added = await run(["add", "-A"], worktree)
  if (added.code !== 0) return added
  return run(["commit", "-m", message], worktree)
}

export type MergeResult = { ok: boolean; message: string }

async function mergeSquash(
  run: GitRunner,
  checkoutDir: string,
  branch: string,
  targetBranch: string,
  commitMessage: string,
  isMainWorktree: boolean,
): Promise<MergeResult> {
  if (isMainWorktree) {
    const status = await run(["status", "--porcelain"], checkoutDir)
    if (status.stdout.trim()) {
      return { ok: false, message: `Commit or stash changes on ${targetBranch} before merging` }
    }
  }
  // `git merge --abort` has no effect after `--squash` (there's no MERGE_HEAD), so `git reset
  // --hard HEAD` is the only way back — safe here only because we just verified the main
  // worktree was clean before starting, so nothing of the user's is lost.
  const fail = async (message: string): Promise<MergeResult> => {
    if (isMainWorktree) await run(["reset", "--hard", "HEAD"], checkoutDir)
    return { ok: false, message }
  }
  const squashed = await run(["merge", "--squash", branch], checkoutDir)
  if (squashed.code !== 0) {
    return fail(squashed.stderr.trim() || squashed.stdout.trim() || `Merge of ${branch} failed`)
  }
  const committed = await run(["commit", "-m", commitMessage], checkoutDir)
  if (committed.code === 0) return { ok: true, message: committed.stdout.trim() || `Merged ${branch}` }
  if (/nothing to commit/i.test(`${committed.stdout}${committed.stderr}`)) {
    return { ok: true, message: "No changes to merge" }
  }
  return fail(committed.stderr.trim() || committed.stdout.trim() || "Commit failed")
}

async function mergeInto(
  run: GitRunner,
  checkoutDir: string,
  branch: string,
  targetBranch: string,
  commitMessage: string,
  squash: boolean,
  isMainWorktree: boolean,
): Promise<MergeResult> {
  if (squash) return mergeSquash(run, checkoutDir, branch, targetBranch, commitMessage, isMainWorktree)
  const merged = await run(["merge", branch], checkoutDir)
  if (merged.code === 0) return { ok: true, message: merged.stdout.trim() || `Merged ${branch}` }
  await run(["merge", "--abort"], checkoutDir)
  return { ok: false, message: merged.stderr.trim() || merged.stdout.trim() || `Merge of ${branch} failed` }
}

export async function mergeTaskBranch(
  run: GitRunner,
  mainWorktree: string,
  taskWorktree: string,
  branch: string,
  targetBranch: string,
  commitMessage: string,
  squash: boolean,
): Promise<MergeResult> {
  const committed = await commitAll(run, taskWorktree, commitMessage)
  if (committed && committed.code !== 0) {
    return { ok: false, message: committed.stderr.trim() || committed.stdout.trim() || "Committing task work failed" }
  }
  const checkedOut = await currentBranch(run, mainWorktree)
  if (checkedOut === targetBranch) {
    return mergeInto(run, mainWorktree, branch, targetBranch, commitMessage, squash, true)
  }
  const tempDir = join(tmpdir(), `kagan-merge-${Math.random().toString(36).slice(2, 8)}`)
  const added = await run(["worktree", "add", tempDir, targetBranch], mainWorktree)
  if (added.code !== 0) {
    return { ok: false, message: added.stderr.trim() || `Cannot check out ${targetBranch} for merging` }
  }
  try {
    return await mergeInto(run, tempDir, branch, targetBranch, commitMessage, squash, false)
  } finally {
    await run(["worktree", "remove", "--force", tempDir], mainWorktree)
    await rm(tempDir, { recursive: true, force: true })
  }
}
