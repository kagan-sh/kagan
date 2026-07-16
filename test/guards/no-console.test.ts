import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"

const srcDir = new URL("../../src/", import.meta.url)

function sourceFiles(directory = srcDir.pathname): string[] {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name)
    if (statSync(path).isDirectory()) return sourceFiles(path)
    return /\.tsx?$/.test(name) && !name.endsWith(".d.ts") ? [path] : []
  })
}

// Server-side console.* writes leak onto the TUI terminal and render over the kanban board.
// Board feedback goes through store.notify / the helperError badge instead. This gate fails if a
// console call is reintroduced anywhere in src.
describe("no console output in src", () => {
  test("no src file uses console.log/error/warn/info/debug", () => {
    const pattern = /\bconsole\.(log|error|warn|info|debug)\b/
    const offenders = sourceFiles()
      .filter((path) => pattern.test(readFileSync(path, "utf8")))
      .map((path) => relative(srcDir.pathname, path))
    expect(offenders).toEqual([])
  })
})
