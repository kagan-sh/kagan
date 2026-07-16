import { KAGAN_PACKAGE, parseRelease } from "./check"

const COMMAND_TIMEOUT_MS = 60_000
const OUTPUT_LIMIT = 2000

type UpdateProcess = {
  exited: Promise<number>
  stdout: ReadableStream<Uint8Array>
  stderr: ReadableStream<Uint8Array>
  kill: (signal?: number | NodeJS.Signals) => void
}

export type UpdateCommandResult = { ok: boolean; output: string; exitCode: number | null }

function bounded(output: string) {
  return output.length > OUTPUT_LIMIT ? output.slice(-OUTPUT_LIMIT) : output
}

export async function runGlobalPluginUpdate(
  version: string,
  cwd: string,
  deps: {
    execPath?: string
    timeoutMs?: number
    spawn?: (command: string[], options: { cwd: string }) => UpdateProcess
  } = {},
): Promise<UpdateCommandResult> {
  const release = parseRelease(version)
  if (!release) return { ok: false, output: "Invalid Kagan release version", exitCode: null }

  let process: UpdateProcess | undefined
  let timedOut = false
  let timer: ReturnType<typeof setTimeout> | undefined
  try {
    const spawn =
      deps.spawn ?? ((command, options) => Bun.spawn(command, { ...options, stdout: "pipe", stderr: "pipe" }))
    process = spawn(
      [deps.execPath ?? globalThis.process.execPath, "plugin", `${KAGAN_PACKAGE}@${release}`, "--global", "--force"],
      { cwd },
    )
    timer = setTimeout(() => {
      timedOut = true
      process?.kill(9)
    }, deps.timeoutMs ?? COMMAND_TIMEOUT_MS)
    const [exitCode, stdout, stderr] = await Promise.all([
      process.exited,
      new Response(process.stdout).text(),
      new Response(process.stderr).text(),
    ])
    const output = bounded(`${stdout}${stderr}`.trim())
    if (timedOut) return { ok: false, output: `Update timed out. ${output}`.trim(), exitCode: null }
    return { ok: exitCode === 0, output, exitCode }
  } catch (error) {
    return { ok: false, output: error instanceof Error ? error.message : String(error), exitCode: null }
  } finally {
    if (timer) clearTimeout(timer)
  }
}
