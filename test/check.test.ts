import { describe, expect, test } from "bun:test"
import { runCheckCommand } from "../src/check"

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
