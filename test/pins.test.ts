import { describe, expect, test } from "bun:test"

describe("dependency pins", () => {
  // .opencode/ is gitignored local config, so the pin can only be enforced where it exists.
  test("the .opencode plugin package matches the root plugin pin", async () => {
    const opencodeFile = Bun.file(new URL("../.opencode/package.json", import.meta.url))
    if (!(await opencodeFile.exists())) return
    const root = await Bun.file(new URL("../package.json", import.meta.url)).json()
    const opencode = await opencodeFile.json()
    expect(opencode.dependencies["@opencode-ai/plugin"]).toBe(root.devDependencies["@opencode-ai/plugin"])
  })
})
