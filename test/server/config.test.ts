import { describe, expect, test } from "bun:test"
import plugin from "../../src/server"
import { buildKaganTaskTemplate } from "../../src/server/command"

describe("server config hook", () => {
  test("injects the kagan-task command with a template ending in $ARGUMENTS", async () => {
    const hooks = await plugin.server({} as never, {})
    const cfg: { command?: Record<string, { template: string; description?: string }> } = {}
    await hooks.config?.(cfg as never)
    expect(cfg.command?.["kagan-task"]?.description).toContain("Kagan board tasks")
    expect(cfg.command?.["kagan-task"]?.template).toBe(buildKaganTaskTemplate({}))
    expect(cfg.command?.["kagan-task"]?.template.trimEnd().endsWith("$ARGUMENTS")).toBe(true)
  })
})
