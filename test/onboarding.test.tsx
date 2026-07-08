/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { maybeShowOnboarding, Onboarding, showOnboarding } from "../src/onboarding"
import { attachRendererMockInput, mockTuiApi } from "./fixtures/api"

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

describe("Onboarding", () => {
  test("renders the logo, welcome label, first step, and dismiss hints", async () => {
    renderSetup = await testRender(() => <Onboarding api={api()} />, { width: 70, height: 24 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("█▄▀")
    expect(frame).toContain("Welcome to the board!")
    expect(frame).toContain("Supervised tasks")
    expect(frame).toContain("x don't show again")
    expect(frame).toContain("esc dismiss")
  })

  test("x persists the opt-out and closes the dialog", async () => {
    const boardApi = api()
    let cleared = false
    ;(boardApi.ui.dialog as unknown as { clear: () => void }).clear = () => {
      cleared = true
    }
    renderSetup = await testRender(() => <Onboarding api={boardApi} />, { width: 70, height: 24 })
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("x")
    await renderSetup.waitFor(() => cleared)
    expect(boardApi.kv.get("kagan:onboarding", false)).toBe(true)
  })

  test("enter closes on the last step", async () => {
    const boardApi = api()
    let cleared = false
    ;(boardApi.ui.dialog as unknown as { clear: () => void }).clear = () => {
      cleared = true
    }
    renderSetup = await testRender(() => <Onboarding api={boardApi} />, { width: 70, height: 24 })
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    for (let i = 0; i < 3; i++) {
      renderSetup.mockInput.pressEnter()
      await renderSetup.flush()
    }
    expect(cleared).toBe(false)
    renderSetup.mockInput.pressEnter()
    await renderSetup.waitFor(() => cleared)
    expect(boardApi.kv.get("kagan:onboarding", false)).toBe(false)
  })
})
