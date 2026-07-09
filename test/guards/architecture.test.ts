import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { dirname, join, relative, resolve } from "node:path"

const srcDir = new URL("../../src/", import.meta.url)

function sourceFiles(directory = srcDir.pathname): string[] {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name)
    if (statSync(path).isDirectory()) return sourceFiles(path)
    return /\.tsx?$/.test(name) ? [path] : []
  })
}

function imports(source: string): string[] {
  return [...source.matchAll(/\bfrom\s+["']([^"']+)["']/g)].map((match) => match[1]!)
}

// The OpenCode host's OpenTUI Solid transform only rewires imports for .jsx/.tsx files; its Bun
// onLoad filter skips plain .ts. A .ts file importing solid-js/@opentui therefore loads the
// snapshot's own bundled Solid instance, orphaned from the host renderer — its signals update but
// the mounted board never repaints. Every file touching those modules must be .tsx (store.tsx is
// the load-bearing case). This gate fails if that regresses.
describe("host Solid bridge", () => {
  test("no src .ts file imports solid-js or @opentui", () => {
    const offenders = sourceFiles().filter(
      (path) =>
        path.endsWith(".ts") &&
        !path.endsWith(".d.ts") &&
        /\bfrom\s+["'](solid-js|@opentui)/.test(readFileSync(path, "utf8")),
    )
    expect(offenders).toEqual([])
  })

  test("src does not import OpenTUI context hooks that bypass TuiPluginApi", () => {
    const offenders = sourceFiles().filter((path) => {
      if (!path.endsWith(".tsx")) return false
      const source = readFileSync(path, "utf8")
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

describe("source architecture", () => {
  test("domain is independent of plugin surfaces", () => {
    const offenders = sourceFiles(join(srcDir.pathname, "domain")).filter((file) =>
      imports(readFileSync(file, "utf8")).some((specifier) => {
        if (!specifier.startsWith(".")) return false
        const target = relative(srcDir.pathname, resolve(dirname(file), specifier))
        return target === "server" || target.startsWith("server/") || target === "tui" || target.startsWith("tui/")
      }),
    )
    expect(offenders).toEqual([])
  })

  test("plugin surfaces do not depend on each other", () => {
    const filesBySurface = {
      server: [join(srcDir.pathname, "server.ts"), ...sourceFiles(join(srcDir.pathname, "server"))],
      tui: [join(srcDir.pathname, "tui.tsx"), ...sourceFiles(join(srcDir.pathname, "tui"))],
    }
    const offenders = (Object.keys(filesBySurface) as Array<keyof typeof filesBySurface>).flatMap((surface) =>
      filesBySurface[surface].filter((file) =>
        imports(readFileSync(file, "utf8")).some((specifier) => {
          if (!specifier.startsWith(".")) return false
          const target = relative(srcDir.pathname, resolve(dirname(file), specifier))
          return surface === "server"
            ? target === "tui" || target.startsWith("tui/")
            : target === "server" || target.startsWith("server/")
        }),
      ),
    )
    expect(offenders).toEqual([])
  })
})
