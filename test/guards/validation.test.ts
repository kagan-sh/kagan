import { describe, expect, test } from "bun:test"

const sourceGlob = '"src/**/*.{ts,tsx}"'

async function text(path: string): Promise<string> {
  return Bun.file(new URL(path, import.meta.url)).text()
}

describe("validation gates", () => {
  test("uses bare Verifyx for fast validation", async () => {
    const packageJson = await Bun.file(new URL("../../package.json", import.meta.url)).json()
    expect(packageJson.scripts.verify).toBe("verifyx")
    expect(packageJson.scripts.verify).not.toContain("verifyx all")
    expect(packageJson.scripts.test).toBe(
      "bun test ./test/checks ./test/domain ./test/git ./test/guards ./test/server ./test/tui --conditions browser && bun test ./test/integration/create-tasks.test.ts --conditions browser",
    )
  })

  test("declares only the requested source checks for fast validation", async () => {
    const packageJson = await Bun.file(new URL("../../package.json", import.meta.url)).json()
    expect(Object.keys(packageJson.scripts).filter((name) => name.startsWith("verify:"))).toEqual([
      "verify:complexity",
      "verify:format",
      "verify:format:fix",
      "verify:comments",
      "verify:circular-deps",
    ])
    expect(packageJson.scripts["verify:complexity"]).toBe(`verifyx complexity --threshold 27 ${sourceGlob}`)
    expect(packageJson.scripts["verify:format"]).toBe("oxfmt --check .")
    expect(packageJson.scripts["verify:format:fix"]).toBe("oxfmt .")
    expect(packageJson.scripts["verify:comments"]).toBe(`verifyx comments --pushback ${sourceGlob}`)
    expect(packageJson.scripts["verify:circular-deps"]).toBe(`verifyx circular-deps -- ${sourceGlob}`)
  })

  test("uses the full check gate in the commit hook and validation workflows", async () => {
    const packageJson = await Bun.file(new URL("../../package.json", import.meta.url)).json()
    expect(packageJson.scripts.check).toBe("verifyx all --check && bun run package")
    expect(Object.keys(packageJson.scripts)).not.toContain("verify:check")
    expect(Object.keys(packageJson.scripts)).not.toContain("verify:package")
    expect((await text("../../.githooks/pre-commit")).trimEnd()).toBe("#!/bin/sh\nset -e\nbun run check")

    for (const workflow of ["../../.github/workflows/check.yml", "../../.github/workflows/release.yml"]) {
      const content = await text(workflow)
      expect(content).toContain("fetch-depth: 0")
      expect(content).toContain("- run: bun run check")
      expect(content).not.toContain("- run: bun run test")
    }
  })
})
