import { join } from "node:path"

export type CommandSpec = {
  name: string
  cwd: string
  command: string
  scope?: string[]
}

export type CommandStepResult = {
  name: string
  cwd: string
  command: string
  status: "ran" | "skipped"
  exitCode: number | null
  output: string
  reason?: string
}

export type CheckResult = { command: string; exitCode: number | null; output: string; steps?: CommandStepResult[] }

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

function aggregateExitCode(steps: CommandStepResult[]): number | null {
  const failed = steps.find((step) => step.status === "ran" && step.exitCode !== 0)
  if (failed) return failed.exitCode
  return 0
}

function summarizeSteps(steps: CommandStepResult[]): string {
  return steps
    .map((step) => {
      if (step.status === "skipped") return `${step.name}: skipped (${step.reason ?? "not in scope"})`
      const exit = step.exitCode === null ? "?" : step.exitCode
      const output = step.output.trim()
      return `${step.name}: exited ${exit}${output ? `\n${output}` : ""}`
    })
    .join("\n\n")
}

export async function runCommandPlan(
  commands: readonly CommandSpec[],
  worktree: string,
  shouldRun: (command: CommandSpec) => boolean,
  skipReason = "no changed files in scope",
  recordSkipped = true,
): Promise<CheckResult | undefined> {
  if (commands.length === 0) return undefined
  const steps: CommandStepResult[] = []
  for (const spec of commands) {
    if (!shouldRun(spec)) {
      if (recordSkipped) {
        steps.push({
          name: spec.name,
          cwd: spec.cwd,
          command: spec.command,
          status: "skipped",
          exitCode: null,
          output: "",
          reason: skipReason,
        })
      }
      continue
    }
    const result = await runCheckCommand(spec.command, join(worktree, spec.cwd))
    steps.push({
      name: spec.name,
      cwd: spec.cwd,
      command: spec.command,
      status: "ran",
      exitCode: result.exitCode,
      output: result.output,
    })
  }
  if (steps.length === 0) return undefined
  return {
    command: steps.map((step) => `${step.name}: ${step.command}`).join(" && "),
    exitCode: aggregateExitCode(steps),
    output: summarizeSteps(steps),
    steps,
  }
}
