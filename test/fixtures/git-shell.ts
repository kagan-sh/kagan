import { execSync } from "node:child_process"
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import type { PluginInput } from "@opencode-ai/plugin"

function cleanGitEnv(): NodeJS.ProcessEnv {
  const env = { ...process.env }
  for (const key of Object.keys(env)) {
    if (key.startsWith("GIT_")) delete env[key]
  }
  env.GIT_CONFIG_GLOBAL = process.env.GIT_CONFIG_GLOBAL ?? "/dev/null"
  env.GIT_CONFIG_SYSTEM = process.env.GIT_CONFIG_SYSTEM ?? "/dev/null"
  return env
}

let gitLock: Promise<void> = Promise.resolve()

async function withGitLock<T>(fn: () => Promise<T>): Promise<T> {
  let release!: () => void
  const gate = new Promise<void>((resolve) => {
    release = resolve
  })
  const previous = gitLock
  gitLock = previous.then(() => gate)
  await previous
  try {
    return await fn()
  } finally {
    release()
  }
}

async function runGit(cwd: string, args: string[]) {
  return withGitLock(async () => {
    const proc = Bun.spawn(["git", "-C", cwd, ...args], {
      env: cleanGitEnv(),
      stdout: "pipe",
      stderr: "pipe",
      stdin: "ignore",
    })
    const [stdout, stderr, exitCode] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ])
    return { exitCode, stdout, stderr }
  })
}

export function initTestRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "kagan-test-repo-"))
  execSync("git init -b main", { cwd: repo, env: cleanGitEnv() })
  execSync('git config user.email "t@test.com" && git config user.name "T"', { cwd: repo, env: cleanGitEnv() })
  execSync("git commit --allow-empty -m init", { cwd: repo, env: cleanGitEnv() })
  return repo
}

export function gitShellForRepo(repo: string): PluginInput["$"] {
  return ((_s: TemplateStringsArray, ...values: unknown[]) => {
    const cwd = String(values[0] ?? repo)
    const argsValue = values[1]
    const args = Array.isArray(argsValue)
      ? argsValue.map(String)
      : typeof argsValue === "string"
        ? argsValue.split(/\s+/).filter(Boolean)
        : []
    return {
      nothrow: () => ({
        quiet: async () => {
          const result = await runGit(cwd, args)
          return {
            exitCode: result.exitCode,
            stdout: Buffer.from(result.stdout),
            stderr: Buffer.from(result.stderr),
          }
        },
      }),
    }
  }) as PluginInput["$"]
}
