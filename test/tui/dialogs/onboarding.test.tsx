/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { maybeShowOnboarding, showOnboarding } from "../../../src/tui/dialogs/onboarding"
import { mockTuiApi } from "../../fixtures/api"

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

function api(options: { seen?: boolean } = {}): TuiPluginApi & { replaces: number } {
  const kvMap: Record<string, unknown> = {}
  if (options.seen) kvMap["kagan:onboarding"] = true
  const result = {
    ...mockTuiApi({ kvMap }),
    replaces: 0,
    ui: {
      toast: () => {},
      dialog: {
        open: false,
        setSize: () => {},
        clear: () => {},
        replace: () => {
          result.replaces++
        },
      },
    },
  } as unknown as TuiPluginApi & { replaces: number }
  return result
}

describe("maybeShowOnboarding", () => {
  test("opens the dialog once per run for an unseen board", () => {
    const boardApi = api()
    expect(maybeShowOnboarding(boardApi)).toBe(true)
    expect(boardApi.replaces).toBe(1)
    expect(maybeShowOnboarding(boardApi)).toBe(false)
    expect(boardApi.replaces).toBe(1)
  })

  test("never opens after the user opted out", () => {
    const boardApi = api({ seen: true })
    expect(maybeShowOnboarding(boardApi)).toBe(false)
    expect(boardApi.replaces).toBe(0)
  })
})

describe("showOnboarding", () => {
  test("opens the tour even after the user opted out", () => {
    const boardApi = api({ seen: true })
    showOnboarding(boardApi)
    showOnboarding(boardApi)
    expect(boardApi.replaces).toBe(2)
  })
})
