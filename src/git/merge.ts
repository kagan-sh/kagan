import { rm } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { currentBranch, type GitResult, type GitRunner } from "./runner"
import { mergeSquash } from "./squash"

async function commitAll(run: GitRunner, worktree: string, message: string): Promise<GitResult | undefined> {
  const status = await run(["status", "--porcelain"], worktree)
  if (status.code !== 0 || !status.stdout.trim()) return undefined
  const added = await run(["add", "-A"], worktree)
  if (added.code !== 0) return added
  return run(["commit", "-m", message], worktree)
}

export type MergeResult = { ok: boolean; message: string }

async function dirtyMainWorktreeMessage(
  run: GitRunner,
  checkoutDir: string,
  targetBranch: string,
): Promise<MergeResult | undefined> {
  const status = await run(["status", "--porcelain"], checkoutDir)
  if (status.stdout.trim()) {
    return { ok: false, message: `Commit or stash changes on ${targetBranch} before merging` }
  }
  return undefined
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
  if (isMainWorktree) {
    const dirty = await dirtyMainWorktreeMessage(run, checkoutDir, targetBranch)
    if (dirty) return dirty
  }
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
