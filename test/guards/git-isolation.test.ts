import { describe, expect, test } from "bun:test"
import { mkdtemp, mkdir, rm, writeFile } from "node:fs/promises"
import { homedir, tmpdir } from "node:os"
import { join } from "node:path"
import { bunGitRunner } from "../../src/git/runner"

describe("git isolation", () => {
  test("production bunGitRunner ignores the user's real global git identity", async () => {
    const run = bunGitRunner
    const repoDir = await mkdtemp(join(tmpdir(), "kagan-isolation-guard-"))
    try {
      expect((await run(["init", "-q", "-b", "main"], repoDir)).code).toBe(0)
      expect((await run(["config", "--get", "user.name"], repoDir)).stdout.trim()).toBe("")
      const globalList = await run(["config", "--global", "--list"], repoDir)
      expect(globalList.stdout.trim()).toBe("")
    } finally {
      await rm(repoDir, { recursive: true, force: true })
    }
  })

  test("git home-dotfile paths under XDG_CONFIG_HOME are isolated from the real user config", async () => {
    const configHome = process.env.XDG_CONFIG_HOME!
    expect(configHome).not.toBe(join(homedir(), ".config"))

    const gitConfigDir = join(configHome, "git")
    await mkdir(gitConfigDir, { recursive: true })
    await writeFile(join(gitConfigDir, "ignore"), "*.secret\n")

    const run = bunGitRunner
    const repoDir = await mkdtemp(join(tmpdir(), "kagan-isolation-xdg-"))
    try {
      await run(["init", "-q", "-b", "main"], repoDir)
      await run(["config", "user.email", "test@kagan.dev"], repoDir)
      await run(["config", "user.name", "Kagan Test"], repoDir)
      await writeFile(join(repoDir, "tracked.secret"), "x\n")
      await writeFile(join(repoDir, "visible.txt"), "y\n")
      await run(["add", "visible.txt"], repoDir)
      await run(["commit", "-q", "-m", "initial"], repoDir)

      const status = await run(["status", "--porcelain"], repoDir)
      expect(status.stdout).not.toContain("tracked.secret")
    } finally {
      await rm(repoDir, { recursive: true, force: true })
    }
  })
})
