import { execSync } from "node:child_process"
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"

async function runGit(cwd: string, args: string[]) {
  const proc = Bun.spawn(["git", "-C", cwd, ...args], {
    env: process.env,
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
}

export function initTestRepo(): string {
  const repo = mkdtempSync(join(tmpdir(), "kagan-test-repo-"))
  execSync("git init -b main", { cwd: repo, env: process.env })
  execSync('git config user.email "t@test.com" && git config user.name "T"', { cwd: repo, env: process.env })
  execSync("git commit --allow-empty -m init", { cwd: repo, env: process.env })
  return repo
}

export async function listBranches(repo: string, pattern: string): Promise<string> {
  const result = await runGit(repo, ["branch", "--list", pattern])
  return result.stdout.trim()
}
