export type CheckResult = { command: string; exitCode: number | null; output: string }

const OUTPUT_LIMIT = 4000

export async function runCheckCommand(command: string, cwd: string, timeoutMs = 300_000): Promise<CheckResult> {
  let proc: ReturnType<typeof Bun.spawn> | undefined
  let timedOut = false
  let timeoutID: ReturnType<typeof setTimeout> | undefined

  try {
    proc = Bun.spawn(["sh", "-c", command], {
      cwd,
      stdout: "pipe",
      stderr: "pipe",
      stdin: "ignore",
      detached: true,
    })

    timeoutID = setTimeout(() => {
      timedOut = true
      if (proc && proc.pid > 0) {
        try {
          // `detached` makes proc.pid the process group leader too, so the negative pid
          // signals the whole group — sh and any background children it spawned — not just sh.
          process.kill(-proc.pid, 9)
        } catch {
          // Process already exited; let the normal path collect output.
        }
      }
    }, timeoutMs)

    await proc.exited

    const stdout = await new Response(proc.stdout as ReadableStream<Uint8Array>).text()
    const stderr = await new Response(proc.stderr as ReadableStream<Uint8Array>).text()
    const combined = stdout + stderr
    const output = combined.length > OUTPUT_LIMIT ? combined.slice(-OUTPUT_LIMIT) : combined

    if (timedOut) {
      return {
        command,
        exitCode: null,
        output: `check timed out after ${timeoutMs / 1000}s\n${output}`,
      }
    }

    return { command, exitCode: proc.exitCode, output }
  } catch (error) {
    return {
      command,
      exitCode: null,
      output: error instanceof Error ? error.message : String(error),
    }
  } finally {
    if (timeoutID !== undefined) clearTimeout(timeoutID)
  }
}
