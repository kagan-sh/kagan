import type { GitRunner, MergeResult } from "./runner"

function hasUserWorkDuringMerge(porcelain: string): boolean {
  for (const line of porcelain.split("\n")) {
    const entry = line.trimEnd()
    if (entry.length < 3 || entry[2] !== " ") continue
    const index = entry[0]!
    const worktree = entry[1]!
    if (index === "?" || worktree === "?") return true
    if (worktree === "M" && index !== "U") return true
    if (worktree === "D" && index !== "U" && index !== "D") return true
  }
  return false
}

export async function mergeSquash(
  run: GitRunner,
  checkoutDir: string,
  branch: string,
  targetBranch: string,
  commitMessage: string,
  isMainWorktree: boolean,
): Promise<MergeResult> {
  let mergeStartedOnCleanMain = false
  if (isMainWorktree) {
    const status = await run(["status", "--porcelain"], checkoutDir)
    if (status.stdout.trim()) {
      return { ok: false, message: `Commit or stash changes on ${targetBranch} before merging` }
    }
    mergeStartedOnCleanMain = true
  }
  const fail = async (message: string): Promise<MergeResult> => {
    if (isMainWorktree && mergeStartedOnCleanMain) {
      const current = await run(["status", "--porcelain"], checkoutDir)
      const out = current.stdout.trimEnd()
      if (!out.trim() || !hasUserWorkDuringMerge(out)) await run(["reset", "--hard", "HEAD"], checkoutDir)
    }
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
