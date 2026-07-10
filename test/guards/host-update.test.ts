import { describe, expect, test } from "bun:test"
import { existsSync, readFileSync } from "node:fs"
import { join } from "node:path"

const hostRuntime = join(import.meta.dir, "../../references/opencode/packages/opencode/src/plugin/tui/runtime.ts")

describe("host update preparation", () => {
  // The vendored host source is git-ignored, so this pin only runs on machines that have it.
  test.skipIf(!existsSync(hostRuntime))("dedupes plugins.add by module id before activation", () => {
    const source = readFileSync(hostRuntime, "utf8")
    expect(source).toMatch(/state\.plugins_by_id\.has\(first\.id\)/)
    expect(source).toMatch(/state\.pending\.delete\(spec\)/)
  })
})
