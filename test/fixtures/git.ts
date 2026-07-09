import { devNull } from "node:os"
import type { GitResult, GitRunner } from "../../src/git/runner"

// Runs the real git binary with every GIT_* variable stripped and global/system config routed to
// the null device, so a test's temp repos can neither read nor write the user's real repo or gitconfig.
function hermeticGitEnv(): Record<string, string> {
  const env: Record<string, string> = {}
  for (const [key, value] of Object.entries(process.env)) {
    if (value !== undefined && !key.startsWith("GIT_")) env[key] = value
  }
  env.GIT_CONFIG_GLOBAL = devNull
  env.GIT_CONFIG_SYSTEM = devNull
  return env
}

export function hermeticGitRunner(): GitRunner {
  const env = hermeticGitEnv()
  return async (args, cwd): Promise<GitResult> => {
    const proc = Bun.spawn(["git", ...args], { cwd, env, stdout: "pipe", stderr: "pipe", stdin: "ignore" })
    const [stdout, stderr, code] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ])
    return { code, stdout, stderr }
  }
}
