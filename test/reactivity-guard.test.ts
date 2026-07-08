import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync } from "node:fs"

// The OpenCode host's OpenTUI Solid transform only rewires imports for .jsx/.tsx files; its Bun
// onLoad filter skips plain .ts. A .ts file importing solid-js/@opentui therefore loads the
// snapshot's own bundled Solid instance, orphaned from the host renderer — its signals update but
// the mounted board never repaints. Every file touching those modules must be .tsx (store.tsx is
// the load-bearing case). This gate fails if that regresses.
describe("host Solid bridge", () => {
  test("no src/*.ts imports solid-js or @opentui — those files must be .tsx", () => {
    const dir = new URL("../src/", import.meta.url)
    const offenders = readdirSync(dir).filter(
      (name) =>
        name.endsWith(".ts") &&
        !name.endsWith(".d.ts") &&
        /\bfrom\s+["'](solid-js|@opentui)/.test(readFileSync(new URL(name, dir), "utf8")),
    )
    expect(offenders).toEqual([])
  })

  test("src does not import OpenTUI context hooks that bypass TuiPluginApi", () => {
    const dir = new URL("../src/", import.meta.url)
    const offenders = readdirSync(dir).filter((name) => {
      if (!name.endsWith(".tsx")) return false
      const source = readFileSync(new URL(name, dir), "utf8")
      return (
        /\bfrom\s+["']@opentui\/keymap\/solid["']/.test(source) ||
        /\bimport\s+\{[^}]*\b(useRenderer|useTerminalDimensions|useKeyboard|Portal)\b[^}]*\}\s+from\s+["']@opentui\/solid["']/.test(
          source,
        )
      )
    })
    expect(offenders).toEqual([])
  })
})
