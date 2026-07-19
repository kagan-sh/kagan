import { describe, expect, test } from "bun:test"
import { buildKaganTaskTemplate } from "../../src/server"

describe("buildKaganTaskTemplate", () => {
  test("ends with $ARGUMENTS", () => {
    const template = buildKaganTaskTemplate()
    expect(template.trimEnd().endsWith("$ARGUMENTS")).toBe(true)
  })

  test("lists configured scope cwd values", () => {
    const template = buildKaganTaskTemplate({
      commands: {
        setup: [
          { name: "alpha", cwd: "project-alpha", command: "npm ci" },
          { name: "beta", cwd: "project-beta", command: "npm ci" },
        ],
      },
    })
    expect(template).toContain("project-alpha")
    expect(template).toContain("project-beta")
    expect(template).toContain("kagan_create_tasks")
  })

  test("guides adaptive source selection from the current session", () => {
    const template = buildKaganTaskTemplate()
    expect(template).toContain("conversation")
    expect(template).toContain("EXACTLY ONCE")
    expect(template).toContain("base the tickets on the conversation so far or only on what they typed")
  })
})
