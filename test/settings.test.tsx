/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, mock, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { TestRendererSetup } from "@opentui/core/testing"
import { Show, createSignal } from "solid-js"
import type { JSX } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { Settings } from "../src/settings"
import { SETTINGS_ROUTE } from "../src/types"
import { mockTuiApi } from "./fixtures/api"

let renderSetups: TestRendererSetup[] = []

afterEach(async () => {
  for (const setup of renderSetups) {
    await setup.renderer.destroy()
  }
  renderSetups = []
  mock.restore()
})

type DialogPromptProps = {
  title: string
  value: string
  placeholder: string
  onConfirm: (value: string) => void
  onCancel: () => void
}

type DialogHarness = {
  api: ReturnType<typeof mockTuiApi>
  renders: (() => unknown)[]
  prompts: DialogPromptProps[]
  lastPrompt: () => DialogPromptProps | undefined
}

function createDialogHarness(overrides: Parameters<typeof mockTuiApi>[0] = {}): DialogHarness {
  const renders: (() => unknown)[] = []
  const prompts: DialogPromptProps[] = []
  const api = mockTuiApi({
    route: { current: { name: SETTINGS_ROUTE } },
    state: { path: { worktree: "/repo" } },
    ui: {
      dialog: {
        open: false,
        replace: (fn: () => unknown) => {
          renders.push(fn)
          ;(api.ui.dialog as { open: boolean }).open = true
        },
        clear: () => {
          renders.length = 0
          prompts.length = 0
          ;(api.ui.dialog as { open: boolean }).open = false
        },
      },
      DialogPrompt: (props: unknown) => {
        prompts.push(props as DialogPromptProps)
        return <box></box>
      },
    },
    ...overrides,
  } as never)
  return { api, renders, prompts, lastPrompt: () => prompts[prompts.length - 1] }
}

function SettingsWithDialog(props: { api: TuiPluginApi; options?: Record<string, unknown>; harness: DialogHarness }) {
  const [activeDialog, setActiveDialog] = createSignal<(() => JSX.Element) | null>(null)
  const originalReplace = props.harness.api.ui.dialog.replace as (fn: () => unknown) => void
  const originalClear = props.harness.api.ui.dialog.clear

  props.harness.api.ui.dialog.replace = (fn: () => unknown) => {
    originalReplace(fn)
    setActiveDialog(() => fn as () => JSX.Element)
  }
  props.harness.api.ui.dialog.clear = () => {
    originalClear()
    setActiveDialog(null)
  }

  return (
    <box position="absolute" left={0} top={0} width="100%" height="100%">
      <Settings api={props.api} options={props.options} />
      <Show when={activeDialog()} keyed>
        {(fn) => (
          <box position="absolute" left={0} top={0} width="100%" height="100%" focused>
            {(fn as () => JSX.Element)()}
          </box>
        )}
      </Show>
    </box>
  )
}

async function renderSettingsWithDialog(options?: Record<string, unknown>) {
  const harness = createDialogHarness()
  const setup = await testRender(() => <SettingsWithDialog api={harness.api} options={options} harness={harness} />, {
    width: 120,
    height: 24,
  })
  await flushAndSettle(setup)
  renderSetups.push(setup)
  return { harness, setup }
}

function confirmPrompt(harness: DialogHarness, value: string) {
  const props = harness.lastPrompt()
  if (!props) throw new Error("No prompt captured")
  props.onConfirm(value)
}

async function flushAndSettle(setup: TestRendererSetup) {
  // Wait a tick after flush so keyed Show remounts can finish onMount
  // (useKeyboard listeners are registered there) before sending the next key.
  await setup.flush()
  await new Promise((resolve) => setTimeout(resolve, 20))
}

function mockFs(contents: Record<string, string> = {}, missing: string[] = []) {
  const files = new Map(Object.entries(contents))
  const missingSet = new Set(missing)
  mock.module("node:fs/promises", () => ({
    readFile: async (path: string) => {
      if (missingSet.has(path)) {
        throw Object.assign(new Error(`ENOENT: ${path}`), { code: "ENOENT" })
      }
      if (files.has(path)) return files.get(path)
      throw Object.assign(new Error(`ENOENT: ${path}`), { code: "ENOENT" })
    },
    writeFile: async (path: string, content: string) => {
      files.set(path, content)
    },
  }))
  return files
}

describe("Settings", () => {
  test("refuses save when opencode.json is missing", async () => {
    mockFs({}, ["/repo/opencode.json"])
    const { setup } = await renderSettingsWithDialog()

    setup.mockInput.pressKey("s")
    await Promise.resolve()
    await flushAndSettle(setup)

    expect(setup.captureCharFrame()).toContain("opencode.json not found in project root")
  })

  test("changing squashMerge updates the JSON preview", async () => {
    const { setup } = await renderSettingsWithDialog()

    for (let i = 0; i < 4; i++) setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    expect(setup.captureCharFrame()).toContain('"squashMerge": true')

    for (let i = 0; i < 4; i++) setup.mockInput.pressKey("ARROW_LEFT")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("ARROW_DOWN")
    setup.mockInput.pressKey("ARROW_DOWN")
    setup.mockInput.pressKey("ARROW_DOWN")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)

    for (let i = 0; i < 4; i++) setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    expect(setup.captureCharFrame()).toContain('"squashMerge": false')
  })

  test("live JSON preview updates", async () => {
    const { harness, setup } = await renderSettingsWithDialog()

    for (let i = 0; i < 4; i++) setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    expect(setup.captureCharFrame()).toContain('"inProgressLimit": 2')

    for (let i = 0; i < 4; i++) setup.mockInput.pressKey("ARROW_LEFT")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)

    confirmPrompt(harness, "5")
    await flushAndSettle(setup)

    for (let i = 0; i < 4; i++) setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    expect(setup.captureCharFrame()).toContain('"inProgressLimit": 5')
  })

  test("editing a setup command and saving", async () => {
    const files = mockFs({
      "/repo/opencode.json": JSON.stringify({ plugin: [["/path/to/kagan", {}]] }, null, 2),
    })
    const { harness, setup } = await renderSettingsWithDialog({
      commands: {
        setup: [{ name: "deps", cwd: ".", command: "bun install" }],
      },
    })

    setup.mockInput.pressKey("ARROW_RIGHT")
    setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)
    await new Promise((resolve) => setTimeout(resolve, 20))

    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)
    await new Promise((resolve) => setTimeout(resolve, 20))

    confirmPrompt(harness, "install deps")
    await flushAndSettle(setup)
    await new Promise((resolve) => setTimeout(resolve, 20))

    setup.mockInput.pressKey("ESCAPE")
    await flushAndSettle(setup)
    await new Promise((resolve) => setTimeout(resolve, 20))
    setup.mockInput.pressKey("s")
    await Promise.resolve()
    await flushAndSettle(setup)
    await new Promise((resolve) => setTimeout(resolve, 10))

    expect(setup.captureCharFrame()).toContain("Saved opencode.json")
    const written = JSON.parse(files.get("/repo/opencode.json") ?? "{}")
    expect(written.plugin[0][1].commands.setup).toEqual([{ name: "install deps", cwd: ".", command: "bun install" }])
  })

  test("adding a validator model", async () => {
    const { harness, setup } = await renderSettingsWithDialog()

    setup.mockInput.pressKey("ARROW_RIGHT")
    setup.mockInput.pressKey("ARROW_RIGHT")
    setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)

    setup.mockInput.pressKey("a")
    await flushAndSettle(setup)
    confirmPrompt(harness, "anthropic")
    confirmPrompt(harness, "claude-4")
    await flushAndSettle(setup)

    setup.mockInput.pressKey("ESCAPE")
    await flushAndSettle(setup)

    setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    expect(setup.captureCharFrame()).toContain('"providerID": "anthropic"')
    expect(setup.captureCharFrame()).toContain('"modelID": "claude-4"')
  })

  test("preserves unrelated plugin entries", async () => {
    const files = mockFs({
      "/repo/opencode.json": JSON.stringify(
        {
          plugin: [
            ["/path/to/kagan", { inProgressLimit: 5 }],
            ["/path/to/other", { setting: true }],
          ],
        },
        null,
        2,
      ),
    })
    const { setup } = await renderSettingsWithDialog()

    setup.mockInput.pressKey("s")
    await Promise.resolve()
    await flushAndSettle(setup)

    const written = JSON.parse(files.get("/repo/opencode.json") ?? "{}")
    expect(written.plugin).toHaveLength(2)
    expect(written.plugin[1]).toEqual(["/path/to/other", { setting: true }])
  })

  test("rejects invalid command input in the editor", async () => {
    const { harness, setup } = await renderSettingsWithDialog()

    setup.mockInput.pressKey("ARROW_RIGHT")
    setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)

    setup.mockInput.pressKey("a")
    await flushAndSettle(setup)
    confirmPrompt(harness, "bad")
    confirmPrompt(harness, "/tmp")
    confirmPrompt(harness, "echo")
    confirmPrompt(harness, "")
    await flushAndSettle(setup)

    expect(setup.captureCharFrame()).toContain("Invalid command")
  })

  test("deletes a command in the editor", async () => {
    const { setup } = await renderSettingsWithDialog({
      commands: {
        setup: [
          { name: "one", cwd: ".", command: "echo one" },
          { name: "two", cwd: ".", command: "echo two" },
        ],
      },
    })

    setup.mockInput.pressKey("ARROW_RIGHT")
    setup.mockInput.pressKey("ARROW_RIGHT")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("RETURN")
    await flushAndSettle(setup)

    setup.mockInput.pressKey("ARROW_DOWN")
    await flushAndSettle(setup)
    setup.mockInput.pressKey("d")
    await flushAndSettle(setup)

    const frame = setup.captureCharFrame()
    expect(frame).toContain("echo one")
    expect(frame).not.toContain("echo two")
  })
})
