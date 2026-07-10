import type { PluginInput } from "@opencode-ai/plugin"
import { mkdir, readFile, writeFile } from "node:fs/promises"
import { homedir } from "node:os"
import { join, resolve } from "node:path"

export type GitResult = { code: number; stdout: string; stderr: string }
export type GitRunner = (args: string[], cwd: string) => Promise<GitResult>
export type MergeResult = { ok: boolean; message: string }

export function bunGitRunner(): GitRunner {
  return async (args, cwd) => {
    const proc = Bun.spawn(["git", ...args], {
      cwd,
      env: process.env,
      stdout: "pipe",
      stderr: "pipe",
      stdin: "ignore",
    })
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

function slugifyTitle(title: string): string {
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

function taskBranch(slug: string): string {
  return `kagan/${slug}`
}

const COMMAND_SEPARATORS = /&&|\|\||[;\n|]/

function isGitToken(token: string): boolean {
  return token === "git" || token.endsWith("/git")
}

function unwrapOuterParens(segment: string): string {
  let s = segment.trim()
  while (s.startsWith("(")) s = s.slice(1).trim()
  while (s.endsWith(")")) s = s.slice(0, -1).trim()
  return s
}

function isGitPushSegment(segment: string): boolean {
  const tokens = unwrapOuterParens(segment).split(/\s+/).filter(Boolean)
  const start = tokens.findIndex(isGitToken)
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

function taskWorktreePath(mainWorktree: string, slug: string): string {
  // KAGAN_WORKTREE_ROOT exists for test isolation (see test/preload/hermetic.ts).
  const root = process.env.KAGAN_WORKTREE_ROOT ?? join(homedir(), ".kagan", "worktrees")
  return join(root, Bun.hash(mainWorktree).toString(16), slug)
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

const kaganPackageRoot = resolve(import.meta.dir, "../..")

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
