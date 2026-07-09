import { describe, expect, test } from "bun:test"
import { readdirSync, readFileSync, statSync } from "node:fs"
import { join, relative } from "node:path"

const testDir = new URL("../", import.meta.url).pathname
const helper = "fixtures/git.ts"

function testFiles(directory = testDir): string[] {
  return readdirSync(directory).flatMap((name) => {
    const path = join(directory, name)
    if (statSync(path).isDirectory()) return testFiles(path)
    return /\.tsx?$/.test(name) ? [path] : []
  })
}

describe("git hermeticity", () => {
  test("no test spawns real git except through the hermetic helper", () => {
    const offenders = testFiles().filter((path) => {
      if (relative(testDir, path) === helper) return false
      const source = readFileSync(path, "utf8")
      return /\bbunGitRunner\(/.test(source) || /Bun\.spawn\(\s*\[\s*["']git["']/.test(source)
    })
    expect(offenders.map((path) => relative(testDir, path))).toEqual([])
  })

  test("no test passes --global or --system to git", () => {
    const offenders = testFiles().filter((path) => /["'](--global|--system)["']/.test(readFileSync(path, "utf8")))
    expect(offenders.map((path) => relative(testDir, path))).toEqual([])
  })
})
