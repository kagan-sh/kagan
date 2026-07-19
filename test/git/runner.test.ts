import { afterEach, describe, expect, test } from "bun:test"
import type { SnapshotFileDiff } from "@opencode-ai/sdk/v2"
import { mkdtemp, mkdir, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import {
  baseBranchFreshness,
  createTaskWorktree,
  ensureWorktreePluginConfig,
  bunGitRunner,
  type GitResult,
  type GitRunner,
  isGitPushCommand,
  uniqueTaskSlug,
} from "../../src/git/runner"
import { mergeTaskBranch } from "../../src/git/merge"
import { orderDiffsByRisk, worktreeDiffs } from "../../src/git/diffs"
import { newSideHunkRanges } from "../../src/domain/task/findings"

function stubRunner(
  handler: (args: string[], cwd: string) => Partial<GitResult> | undefined,
  calls?: string[][],
): GitRunner {
  return async (args, cwd) => {
    calls?.push(args)
    return { code: 0, stdout: "", stderr: "", ...handler(args, cwd) }
  }
}

describe("uniqueTaskSlug", () => {
  test("appends a random suffix", () => {
    const slug = uniqueTaskSlug("Fix bug")
    expect(slug).toMatch(/^fix-bug-[a-z0-9]{4}$/)
  })
})

describe("baseBranchFreshness", () => {
  test("reports the number of commits the base branch is ahead of HEAD", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "rev-list" && args.includes("HEAD..main")) return { stdout: "3\n" }
      return undefined
    })
    expect(await baseBranchFreshness(run, "/wt", "main")).toEqual({ ahead: 3 })
  })

  test("reports zero when the base branch is even with HEAD", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "rev-list" && args.includes("HEAD..main")) return { stdout: "0\n" }
      return undefined
    })
    expect(await baseBranchFreshness(run, "/wt", "main")).toEqual({ ahead: 0 })
  })

  test("returns zero when the base branch is missing", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "rev-list") return { code: 1, stderr: "bad revision 'main'" }
      return undefined
    })
    expect(await baseBranchFreshness(run, "/wt", "main")).toEqual({ ahead: 0 })
  })

  test("returns zero when the worktree is missing", async () => {
    const run = stubRunner(() => undefined)
    expect(await baseBranchFreshness(run, undefined, "main")).toEqual({ ahead: 0 })
  })

  test("returns zero when the base branch name is not recorded", async () => {
    const run = stubRunner(() => undefined)
    expect(await baseBranchFreshness(run, "/wt", undefined)).toEqual({ ahead: 0 })
  })

  test("returns zero for non-numeric command output", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "rev-list") return { stdout: "unexpected\n" }
      return undefined
    })
    expect(await baseBranchFreshness(run, "/wt", "main")).toEqual({ ahead: 0 })
  })
})

describe("isGitPushCommand", () => {
  test.each([
    ["git push", true],
    ["git push --force", true],
    ["git push origin HEAD", true],
    ["git -C somewhere push", true],
    ["git -c http.extraHeader=x push origin HEAD", true],
    ["/usr/bin/git push", true],
    ["/usr/bin/git push origin main", true],
    ["sudo /usr/bin/git push", true],
    ["GIT_SSH_COMMAND='ssh -i key' git push origin main", true],
    ["cd worktree && git push", true],
    ["(git push)", true],
    ["(git push && true)", true],
    ["(git push; true)", true],
    ["(git push origin main)", true],
    ["( /usr/bin/git push )", true],
    ["(git status)", false],
    ["git commit -m wip; git push", true],
    ["git commit -m wip\ngit push", true],
    ["git pull", false],
    ["git fetch origin", false],
    ["git log --oneline", false],
    ["npm run build", false],
    ["echo hello", false],
  ])("%s -> %s", (command, expected) => {
    expect(isGitPushCommand(command)).toBe(expected)
  })
})

describe("newSideHunkRanges", () => {
  test("extracts a range from a single hunk with explicit counts", () => {
    expect(newSideHunkRanges("@@ -1,3 +1,3 @@\n context\n-old\n+new\n")).toEqual([{ start: 1, end: 4 }])
  })

  test("collects a range per hunk across multiple hunks", () => {
    const patch = "@@ -1,2 +1,2 @@\n-a\n+b\n@@ -10,2 +10,4 @@\n+c\n+d\n context\n context\n"
    expect(newSideHunkRanges(patch)).toEqual([
      { start: 1, end: 3 },
      { start: 10, end: 14 },
    ])
  })

  test("defaults an omitted new-side count to a single-line range", () => {
    expect(newSideHunkRanges("@@ -1 +1 @@\n-old\n+new\n")).toEqual([{ start: 1, end: 2 }])
  })

  test("produces an empty range for a zero-count new side (pure deletion hunk)", () => {
    expect(newSideHunkRanges("@@ -5,3 +4,0 @@\n-a\n-b\n-c\n")).toEqual([{ start: 4, end: 4 }])
  })

  test("covers the whole file for an added file's single hunk", () => {
    expect(newSideHunkRanges("@@ -0,0 +1,5 @@\n+a\n+b\n+c\n+d\n+e\n")).toEqual([{ start: 1, end: 6 }])
  })

  test("returns an empty array when there is no patch text", () => {
    expect(newSideHunkRanges("")).toEqual([])
  })
})

const trackedPatch =
  "diff --git a/src/z.ts b/src/z.ts\n@@ -1 +1 @@\n-old\n+new\n" +
  "diff --git a/src/a.ts b/src/a.ts\nnew file mode 100644\n@@ -0,0 +1,2 @@\n+one\n+two\n"

function diffRunner(): GitRunner {
  return stubRunner((args) => {
    if (args[0] === "merge-base") return { stdout: "basesha\n" }
    if (args.includes("--numstat")) return { stdout: "1\t0\tsrc/z.ts\n2\t3\tsrc/a.ts\n" }
    if (args.includes("--name-status")) return { stdout: "M\tsrc/z.ts\nA\tsrc/a.ts\n" }
    if (args.includes("--patch")) return { stdout: trackedPatch }
    if (args[0] === "ls-files") return { stdout: "src/new.ts\n" }
    if (args.includes("--no-index")) return { stdout: "diff\n+alpha\n+beta\n" }
    return undefined
  })
}

describe("worktreeDiffs", () => {
  test("assembles tracked and untracked diffs sorted by file", async () => {
    const diffs = await worktreeDiffs(diffRunner(), "/wt", "main")
    expect(diffs.map((d) => d.file)).toEqual(["src/a.ts", "src/new.ts", "src/z.ts"])
    const a = diffs.find((d) => d.file === "src/a.ts")!
    expect(a).toMatchObject({ additions: 2, deletions: 3, status: "added" })
    expect(a.patch).toContain("+two")
    const untracked = diffs.find((d) => d.file === "src/new.ts")!
    expect(untracked).toMatchObject({ additions: 2, deletions: 0, status: "added" })
  })

  test("returns empty when no diff base can be resolved", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "merge-base") return { code: 1 }
      if (args[0] === "rev-parse") return { code: 1 }
      return undefined
    })
    expect(await worktreeDiffs(run, "/wt", "main")).toEqual([])
  })

  test("treats a binary numstat line (-\\t-) as zero additions and deletions", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "merge-base") return { stdout: "basesha\n" }
      if (args.includes("--numstat")) return { stdout: "-\t-\timg.bin\n" }
      if (args.includes("--name-status")) return { stdout: "M\timg.bin\n" }
      if (args.includes("--patch")) return { stdout: "" }
      if (args[0] === "ls-files") return { stdout: "" }
      return undefined
    })
    const diffs = await worktreeDiffs(run, "/wt", "main")
    expect(diffs.find((d) => d.file === "img.bin")).toMatchObject({ additions: 0, deletions: 0 })
  })

  test("falls back to modified for an unrecognized single-letter name-status code", async () => {
    const run = stubRunner((args) => {
      if (args[0] === "merge-base") return { stdout: "basesha\n" }
      if (args.includes("--numstat")) return { stdout: "5\t2\tsrc/typechange.ts\n" }
      if (args.includes("--name-status")) return { stdout: "T\tsrc/typechange.ts\n" }
      if (args.includes("--patch")) return { stdout: "" }
      if (args[0] === "ls-files") return { stdout: "" }
      return undefined
    })
    const diffs = await worktreeDiffs(run, "/wt", "main")
    expect(diffs.find((d) => d.file === "src/typechange.ts")?.status).toBe("modified")
  })
})

describe("createTaskWorktree", () => {
  const created: string[] = []

  afterEach(async () => {
    await Promise.all(created.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
  })

  test("truncates a long title's slug to at most 40 chars in the kagan/ branch", async () => {
    const calls: string[][] = []
    const run = stubRunner(() => ({ code: 0 }), calls)
    const mainWorktree = join(tmpdir(), "kagan-branch-main")
    const worktreeRoot = process.env.KAGAN_WORKTREE_ROOT!
    created.push(join(worktreeRoot, Bun.hash(mainWorktree).toString(16)))
    const slug = uniqueTaskSlug("Implement a really long feature title that clearly exceeds forty characters")

    await createTaskWorktree(run, mainWorktree, slug, "main")

    const addCall = calls.find((c) => c[0] === "worktree" && c[1] === "add")!
    const branch = addCall[addCall.indexOf("-b") + 1]!
    expect(branch.startsWith("kagan/")).toBe(true)
    expect(addCall.at(-2)).toMatch(new RegExp(`^${worktreeRoot.replaceAll("/", "\\/")}/`))
    const slugPortion = branch.slice("kagan/".length).replace(/-[a-z0-9]{4}$/, "")
    expect(slugPortion.length).toBeLessThanOrEqual(40)
    expect(slugPortion.length).toBeGreaterThan(30)
  })
})

describe("orderDiffsByRisk", () => {
  function diff(file: string, additions: number, deletions: number): SnapshotFileDiff {
    return { file, patch: "", additions, deletions, status: "modified" }
  }

  test("orders risk-boosted files before trivial ones and breaks ties alphabetically", () => {
    const ordered = orderDiffsByRisk([
      diff("src/app.ts", 50, 50),
      diff("src/app.test.ts", 2, 2),
      diff("package.json", 3, 1),
      diff("tsconfig.json", 1, 0),
      diff("bun.lock", 100, 100),
      diff("README.md", 1, 0),
    ])
    expect(ordered.map((d) => d.file)).toEqual([
      "bun.lock",
      "package.json",
      "src/app.test.ts",
      "tsconfig.json",
      "src/app.ts",
      "README.md",
    ])
  })

  test("uses alphabetical path as a stable tie-breaker", () => {
    const ordered = orderDiffsByRisk([diff("z.ts", 10, 0), diff("a.ts", 10, 0), diff("m.ts", 10, 0)])
    expect(ordered.map((d) => d.file)).toEqual(["a.ts", "m.ts", "z.ts"])
  })
})

describe("mergeTaskBranch", () => {
  test("merges directly in the main worktree when it is already on the target branch", async () => {
    const calls: string[][] = []
    const run = stubRunner((args) => {
      if (args[0] === "status") return { stdout: "" }
      if (args[0] === "branch" && args.includes("--show-current")) return { stdout: "main\n" }
      if (args[0] === "merge") return { stdout: "Fast-forward" }
      return undefined
    }, calls)
    const result = await mergeTaskBranch(run, "/main", "/wt", "kagan/x", "main", "kagan: task", false)
    expect(result.ok).toBe(true)
    expect(calls.some((c) => c[0] === "worktree" && c[1] === "add")).toBe(false)
    expect(calls.some((c) => c[0] === "merge" && c[1] === "kagan/x")).toBe(true)
  })

  test("checks out the target in a temp worktree when it is not the current branch", async () => {
    const calls: string[][] = []
    const run = stubRunner((args) => {
      if (args[0] === "status") return { stdout: "" }
      if (args[0] === "branch" && args.includes("--show-current")) return { stdout: "feature\n" }
      if (args[0] === "merge") return { stdout: "Merged" }
      return undefined
    }, calls)
    const result = await mergeTaskBranch(run, "/main", "/wt", "kagan/x", "main", "kagan: task", false)
    expect(result.ok).toBe(true)
    const addCall = calls.find((c) => c[0] === "worktree" && c[1] === "add")
    expect(addCall).toContain("main")
    expect(calls.some((c) => c[0] === "worktree" && c[1] === "remove")).toBe(true)
  })

  test("aborts and reports failure when the merge fails", async () => {
    const calls: string[][] = []
    const run = stubRunner((args) => {
      if (args[0] === "status") return { stdout: "" }
      if (args[0] === "branch" && args.includes("--show-current")) return { stdout: "main\n" }
      if (args[0] === "merge" && args.includes("--abort")) return {}
      if (args[0] === "merge") return { code: 1, stderr: "conflict in src/a.ts" }
      return undefined
    }, calls)
    const result = await mergeTaskBranch(run, "/main", "/wt", "kagan/x", "main", "kagan: task", false)
    expect(result.ok).toBe(false)
    expect(result.message).toContain("conflict")
    expect(calls.some((c) => c[0] === "merge" && c[1] === "--abort")).toBe(true)
  })

  test("reports a failed commit without attempting a merge", async () => {
    const calls: string[][] = []
    const run = stubRunner((args) => {
      if (args[0] === "status") return { stdout: " M src/a.ts\n" }
      if (args[0] === "add") return {}
      if (args[0] === "commit") return { code: 1, stderr: "commit hook failed" }
      return undefined
    }, calls)
    const result = await mergeTaskBranch(run, "/main", "/wt", "kagan/x", "main", "kagan: task", false)
    expect(result.ok).toBe(false)
    expect(result.message).toContain("commit hook failed")
    expect(calls.some((c) => c[0] === "merge")).toBe(false)
  })

  test("squash merge failure skips reset when the main worktree became dirty during merge", async () => {
    let mainStatusCalls = 0
    const calls: string[][] = []
    const run = stubRunner((args, cwd) => {
      if (args[0] === "status" && cwd === "/main") {
        mainStatusCalls++
        return { stdout: mainStatusCalls === 1 ? "" : " M src/a.ts\n" }
      }
      if (args[0] === "status") return { stdout: "" }
      if (args[0] === "branch" && args.includes("--show-current")) return { stdout: "main\n" }
      if (args[0] === "merge" && args.includes("--squash")) return { code: 1, stderr: "conflict" }
      return undefined
    }, calls)
    const result = await mergeTaskBranch(run, "/main", "/wt", "kagan/x", "main", "kagan: task", true)
    expect(result.ok).toBe(false)
    expect(result.message).toContain("conflict")
    expect(calls.filter((c) => c[0] === "reset")).toEqual([])
  })
})

describe("mergeTaskBranch (real repo)", () => {
  const run = bunGitRunner
  const tempDirs: string[] = []

  afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
  })

  async function createRepo(): Promise<string> {
    const dir = await mkdtemp(join(tmpdir(), "kagan-git-repo-"))
    tempDirs.push(dir)
    await run(["init", "-q", "-b", "main"], dir)
    await run(["config", "user.email", "test@kagan.dev"], dir)
    await run(["config", "user.name", "Kagan Test"], dir)
    await writeFile(join(dir, "shared.txt"), "line1\n")
    await run(["add", "-A"], dir)
    await run(["commit", "-q", "-m", "initial"], dir)
    return dir
  }

  async function addTaskWorktree(mainDir: string, branch: string): Promise<string> {
    const dir = await mkdtemp(join(tmpdir(), "kagan-git-task-"))
    tempDirs.push(dir)
    await rm(dir, { recursive: true, force: true })
    await run(["worktree", "add", "-q", "-b", branch, dir, "main"], mainDir)
    return dir
  }

  async function headSha(dir: string): Promise<string> {
    return (await run(["rev-parse", "HEAD"], dir)).stdout.trim()
  }

  async function commitCount(dir: string): Promise<number> {
    return Number((await run(["rev-list", "--count", "HEAD"], dir)).stdout.trim())
  }

  async function commitMessages(dir: string, count: number): Promise<string[]> {
    const result = await run(["log", `-${count}`, "--format=%s"], dir)
    return result.stdout.trim().split("\n")
  }

  async function commitToBranch(dir: string, file: string, content: string, message: string): Promise<void> {
    await writeFile(join(dir, file), content)
    await run(["add", "-A"], dir)
    await run(["commit", "-q", "-m", message], dir)
  }

  test("squash merge of a branch with multiple commits lands exactly one new commit", async () => {
    const mainDir = await createRepo()
    const taskDir = await addTaskWorktree(mainDir, "kagan/multi")
    await commitToBranch(taskDir, "feature-a.txt", "a\n", "add feature a")
    await commitToBranch(taskDir, "feature-b.txt", "b\n", "add feature b")

    const before = await commitCount(mainDir)
    const result = await mergeTaskBranch(run, mainDir, taskDir, "kagan/multi", "main", "kagan: multi", true)

    expect(result.ok).toBe(true)
    expect(await commitCount(mainDir)).toBe(before + 1)
    const [subject] = await commitMessages(mainDir, 1)
    expect(subject).toBe("kagan: multi")
    expect(await Bun.file(join(mainDir, "feature-a.txt")).text()).toBe("a\n")
    expect(await Bun.file(join(mainDir, "feature-b.txt")).text()).toBe("b\n")
  })

  test("non-squash merge preserves the branch's individual commits", async () => {
    const mainDir = await createRepo()
    const taskDir = await addTaskWorktree(mainDir, "kagan/preserve")
    await commitToBranch(taskDir, "feature-a.txt", "a\n", "add feature a")
    await commitToBranch(taskDir, "feature-b.txt", "b\n", "add feature b")

    const before = await commitCount(mainDir)
    const result = await mergeTaskBranch(run, mainDir, taskDir, "kagan/preserve", "main", "kagan: preserve", false)

    expect(result.ok).toBe(true)
    expect(await commitCount(mainDir)).toBe(before + 2)
    expect(await commitMessages(mainDir, 2)).toEqual(["add feature b", "add feature a"])
  })

  test("squash merge with no net change is a successful no-op that adds no commit", async () => {
    const mainDir = await createRepo()
    const taskDir = await addTaskWorktree(mainDir, "kagan/noop")

    const before = await headSha(mainDir)
    const result = await mergeTaskBranch(run, mainDir, taskDir, "kagan/noop", "main", "kagan: noop", true)

    expect(result).toEqual({ ok: true, message: "No changes to merge" })
    expect(await headSha(mainDir)).toBe(before)
  })

  test("squash conflict on a dirty main worktree refuses and leaves the tree untouched", async () => {
    const mainDir = await createRepo()
    const taskDir = await addTaskWorktree(mainDir, "kagan/dirty")
    await commitToBranch(taskDir, "shared.txt", "task-version\n", "task change")
    await writeFile(join(mainDir, "shared.txt"), "dirty-uncommitted\n")

    const before = await headSha(mainDir)
    const result = await mergeTaskBranch(run, mainDir, taskDir, "kagan/dirty", "main", "kagan: dirty", true)

    expect(result).toEqual({ ok: false, message: "Commit or stash changes on main before merging" })
    expect(await headSha(mainDir)).toBe(before)
    expect(await Bun.file(join(mainDir, "shared.txt")).text()).toBe("dirty-uncommitted\n")
  })

  test("non-squash merge on a dirty main worktree refuses and leaves the tree untouched", async () => {
    const mainDir = await createRepo()
    const taskDir = await addTaskWorktree(mainDir, "kagan/dirty-ns")
    await commitToBranch(taskDir, "shared.txt", "task-version\n", "task change")
    await writeFile(join(mainDir, "shared.txt"), "dirty-uncommitted\n")

    const before = await headSha(mainDir)
    const result = await mergeTaskBranch(run, mainDir, taskDir, "kagan/dirty-ns", "main", "kagan: dirty-ns", false)

    expect(result).toEqual({ ok: false, message: "Commit or stash changes on main before merging" })
    expect(await headSha(mainDir)).toBe(before)
    expect(await Bun.file(join(mainDir, "shared.txt")).text()).toBe("dirty-uncommitted\n")
  })

  test("squash conflict on a clean main worktree restores HEAD with no conflict markers", async () => {
    const mainDir = await createRepo()
    const taskDir = await addTaskWorktree(mainDir, "kagan/conflict")
    await commitToBranch(taskDir, "shared.txt", "task-version\n", "task change")
    await commitToBranch(mainDir, "shared.txt", "main-version\n", "main change")

    const before = await headSha(mainDir)
    const result = await mergeTaskBranch(run, mainDir, taskDir, "kagan/conflict", "main", "kagan: conflict", true)

    expect(result.ok).toBe(false)
    expect(await headSha(mainDir)).toBe(before)
    const status = await run(["status", "--porcelain"], mainDir)
    expect(status.stdout.trim()).toBe("")
    expect(await Bun.file(join(mainDir, "shared.txt")).text()).toBe("main-version\n")
  })
})

describe("ensureWorktreePluginConfig", () => {
  const tempDirs: string[] = []

  afterEach(async () => {
    await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
  })

  async function makeWorktree(): Promise<string> {
    const dir = await mkdtemp(join(tmpdir(), "kagan-worktree-config-"))
    tempDirs.push(dir)
    return dir
  }

  async function readConfig(dir: string): Promise<Record<string, unknown>> {
    return JSON.parse(await readFile(join(dir, ".opencode", "opencode.json"), "utf8"))
  }

  test("creates .opencode/opencode.json registering the plugin in a bare worktree", async () => {
    const dir = await makeWorktree()
    await ensureWorktreePluginConfig(dir, "/plugins/kagan")
    expect(await readConfig(dir)).toEqual({
      $schema: "https://opencode.ai/config.json",
      plugin: ["/plugins/kagan"],
    })
  })

  test("defaults the plugin spec to this package's root", async () => {
    const dir = await makeWorktree()
    await ensureWorktreePluginConfig(dir)
    const config = await readConfig(dir)
    const spec = (config.plugin as string[])[0]!
    expect(JSON.parse(await readFile(join(spec, "package.json"), "utf8")).name).toBe("@kagan-sh/kagan")
  })

  test("appends to an existing config without touching other plugins or settings", async () => {
    const dir = await makeWorktree()
    await mkdir(join(dir, ".opencode"), { recursive: true })
    await writeFile(
      join(dir, ".opencode", "opencode.json"),
      JSON.stringify({ plugin: ["their-plugin"], theme: "dark" }),
    )
    await ensureWorktreePluginConfig(dir, "/plugins/kagan")
    expect(await readConfig(dir)).toEqual({
      plugin: ["their-plugin", "/plugins/kagan"],
      theme: "dark",
    })
  })

  test("is a no-op when the plugin is already registered", async () => {
    const dir = await makeWorktree()
    await ensureWorktreePluginConfig(dir, "/plugins/kagan")
    await ensureWorktreePluginConfig(dir, "/plugins/kagan")
    expect((await readConfig(dir)).plugin).toEqual(["/plugins/kagan"])
  })

  test("throws instead of clobbering a config it cannot parse", async () => {
    const dir = await makeWorktree()
    await mkdir(join(dir, ".opencode"), { recursive: true })
    await writeFile(join(dir, ".opencode", "opencode.json"), "{ not json")
    await expect(ensureWorktreePluginConfig(dir, "/plugins/kagan")).rejects.toThrow("not valid JSON")
    expect(await readFile(join(dir, ".opencode", "opencode.json"), "utf8")).toBe("{ not json")
  })
})
