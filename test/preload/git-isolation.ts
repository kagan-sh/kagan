import { mkdtempSync } from "node:fs"
import { devNull, tmpdir } from "node:os"
import { join } from "node:path"

// Git once leaked through tests into the developer's real repo (922d104). Scrub GIT_* process-wide
// so every spawn — including src-internal bunGitRunner() — inherits an isolated env.
const isolatedHome = mkdtempSync(join(tmpdir(), "kagan-test-home-"))
process.env.XDG_CONFIG_HOME = join(isolatedHome, "config")
process.env.KAGAN_WORKTREE_ROOT = mkdtempSync(join(tmpdir(), "kagan-test-worktrees-"))

for (const key of Object.keys(process.env)) {
  if (key.startsWith("GIT_")) delete process.env[key]
}
delete process.env.GIT_INDEX_FILE

process.env.GIT_CONFIG_GLOBAL = devNull
process.env.GIT_CONFIG_SYSTEM = devNull
