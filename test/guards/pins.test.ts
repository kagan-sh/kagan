import { describe, expect, test } from "bun:test"
import { satisfies, valid } from "semver"

describe("dependency pins", () => {
  test("OpenCode plugin and SDK pins are exact, synchronized, and inside the supported engine range", async () => {
    const root = await Bun.file(new URL("../../package.json", import.meta.url)).json()
    const plugin = root.dependencies["@opencode-ai/plugin"]
    const sdk = root.devDependencies["@opencode-ai/sdk"]
    expect(valid(plugin)).toBe(plugin)
    expect(valid(sdk)).toBe(sdk)
    expect(plugin).toBe(sdk)
    expect(satisfies(plugin, root.engines.opencode)).toBe(true)
  })

  // .opencode/ is gitignored local config, so the pin can only be enforced where it exists.
  test("the .opencode plugin package matches the root plugin pin", async () => {
    const opencodeFile = Bun.file(new URL("../../.opencode/package.json", import.meta.url))
    if (!(await opencodeFile.exists())) return
    const root = await Bun.file(new URL("../../package.json", import.meta.url)).json()
    const opencode = await opencodeFile.json()
    expect(opencode.dependencies["@opencode-ai/plugin"]).toBe(root.dependencies["@opencode-ai/plugin"])
  })
})
