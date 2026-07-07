/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { TestRendererSetup } from "@opentui/core/testing"
import { Settings } from "../src/settings"
import { SETTINGS_ROUTE } from "../src/types"
import { mockTuiApi } from "./fixtures/api"

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

describe("Settings", () => {
  test("rejects invalid validatorModels before saving", async () => {
    const api = mockTuiApi({
      route: { current: { name: SETTINGS_ROUTE } },
      state: { path: { worktree: "/repo" } },
      ui: { dialog: { open: false } },
    } as never)

    renderSetup = await testRender(
      () => <Settings api={api} options={{ validatorModels: [{ providerID: "anthropic" }] }} />,
      { width: 120, height: 24 },
    )
    await renderSetup.flush()

    renderSetup.mockInput.pressKey("s")
    await Promise.resolve()
    await renderSetup.flush()

    expect(renderSetup.captureCharFrame()).toContain("validatorModels[0] must be")
  })
})
