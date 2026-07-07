import { afterEach, describe, expect, test } from "bun:test"
import { mkdtemp, rm } from "node:fs/promises"
import { join } from "node:path"
import { tmpdir } from "node:os"
import { runCheckCommand, runCommandPlan } from "../src/check"

const tempDirs: string[] = []

afterEach(async () => {
  await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
})

async function tempWorktree(): Promise<string> {
  const dir = await mkdtemp(join(tmpdir(), "kagan-check-"))
  tempDirs.push(dir)
  return dir
}

describe("runCheckCommand", () => {
  test("captures exit code and combined stdout + stderr", async () => {
    const result = await runCheckCommand('echo "hello"; echo "err" >&2', "/tmp")
    expect(result.exitCode).toBe(0)
    expect(result.output).toContain("hello")
    expect(result.output).toContain("err")
  })

  test("keeps only the last 4000 characters of output", async () => {
    const result = await runCheckCommand("yes '*' | head -n 5000 | tr -d '\\n'", "/tmp")
    expect(result.output).toHaveLength(4000)
  })

  test("records a non-zero exit code honestly", async () => {
    const result = await runCheckCommand('echo "warn" >&2; exit 7', "/tmp")
    expect(result.exitCode).toBe(7)
    expect(result.output).toContain("warn")
  })

  test("returns exitCode null on timeout without throwing", async () => {
    const result = await runCheckCommand("sleep 5", "/tmp", 1)
    expect(result.exitCode).toBeNull()
    expect(result.output).toContain("timed out")
  })

  test("keeps captured output on timeout", async () => {
    const result = await runCheckCommand("echo partial; exec sleep 5", "/tmp", 50)
    expect(result.exitCode).toBeNull()
    expect(result.output).toContain("timed out")
    expect(result.output).toContain("partial")
  })

  test("resolves by the deadline even when a background child outlives the killed shell", async () => {
    const start = Date.now()
    const result = await runCheckCommand("sh -c 'sleep 30 & sleep 30'", "/tmp", 500)
    expect(Date.now() - start).toBeLessThan(5000)
    expect(result.exitCode).toBeNull()
    expect(result.output).toContain("timed out")
  })

  test("returns exitCode null on an unspawnable command without throwing", async () => {
    const result = await runCheckCommand("echo ok", "/definitely-not-a-real-directory")
    expect(result.exitCode).toBeNull()
    expect(result.output.length).toBeGreaterThan(0)
  })
})

describe("runCommandPlan", () => {
  test("records skipped commands as evidence", async () => {
    const worktree = await tempWorktree()
    const result = await runCommandPlan(
      [
        { name: "member", cwd: ".", command: "echo member" },
        { name: "coach", cwd: ".", command: "echo coach" },
      ],
      worktree,
      (command) => command.name === "member",
    )
    expect(result?.exitCode).toBe(0)
    expect(result?.steps).toEqual([
      { name: "member", cwd: ".", command: "echo member", status: "ran", exitCode: 0, output: "member\n" },
      {
        name: "coach",
        cwd: ".",
        command: "echo coach",
        status: "skipped",
        exitCode: null,
        output: "",
        reason: "no changed files in scope",
      },
    ])
  })

  test("continues after a failing command", async () => {
    const worktree = await tempWorktree()
    const result = await runCommandPlan(
      [
        { name: "fail", cwd: ".", command: "exit 4" },
        { name: "next", cwd: ".", command: "echo next" },
      ],
      worktree,
      () => true,
    )
    expect(result?.exitCode).toBe(4)
    expect(result?.steps?.map((step) => step.status)).toEqual(["ran", "ran"])
    expect(result?.output).toContain("next")
  })
})
