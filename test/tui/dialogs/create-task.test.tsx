/** @jsxImportSource @opentui/solid */
import { afterEach, beforeEach, describe, expect, mock, spyOn, test } from "bun:test"
import { testRender } from "@opentui/solid"
import { EditBufferRenderable } from "@opentui/core"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { KeyEvent, TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { JSX } from "solid-js"
import { attachRendererMockInput, mockTuiApi } from "../../fixtures/api"

let branches = ["main", "feature"]
let createInput: Record<string, unknown> | undefined
let createError: Error | undefined
let backlogOrder = ["old"]
let writtenOrder: string[] | undefined
let mockCreateTask = false

const realGit = await import("../../../src/git/runner")
const realTasks = await import("../../../src/tui/tasks")
const realPreferences = await import("../../../src/tui/session/preferences")
const realCreateTask = realTasks.createTask
const realGetOrder = realPreferences.getOrder
const realSetOrder = realPreferences.setOrder
const realListLocalBranches = realGit.listLocalBranches

mock.module("../../../src/git/runner", () => ({
  ...realGit,
  listLocalBranches: async (...args: Parameters<typeof realListLocalBranches>) =>
    mockCreateTask ? branches : realListLocalBranches(...args),
}))

mock.module("../../../src/tui/tasks", () => ({
  ...realTasks,
  createTask: async (...args: Parameters<typeof realCreateTask>) => {
    if (!mockCreateTask) return realCreateTask(...args)
    if (createError) throw createError
    createInput = args[1] as Record<string, unknown>
    return { id: "new1" } as never
  },
}))

mock.module("../../../src/tui/session/preferences", () => ({
  ...realPreferences,
  getOrder: (...args: Parameters<typeof realGetOrder>) => (mockCreateTask ? backlogOrder : realGetOrder(...args)),
  setOrder: (...args: Parameters<typeof realSetOrder>) => {
    if (!mockCreateTask) return realSetOrder(...args)
    writtenOrder = [...args[2]]
  },
}))

const { openCreateTaskDialog } = await import("../../../src/tui/dialogs/create-task")

type Notice = { variant: string; title: string; message: string }

let renderSetup: TestRendererSetup | undefined

beforeEach(() => {
  mockCreateTask = true
  branches = ["main", "feature"]
  createInput = undefined
  createError = undefined
  backlogOrder = ["old"]
  writtenOrder = undefined
})

afterEach(async () => {
  mockCreateTask = false
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

function harness(scopes: string[] = []) {
  const notices: Notice[] = []
  const renders: Array<() => JSX.Element> = []
  let cleared = false
  let refreshes = 0
  const api = mockTuiApi({
    state: {
      path: { worktree: "/repo" },
      vcs: { branch: "feature", default_branch: "main" },
      provider: [{ id: "anthropic", models: { sonnet: { id: "sonnet" } } }],
    },
    ui: {
      dialog: {
        open: false,
        setSize: () => {},
        clear: () => {
          cleared = true
        },
        replace: (render: () => JSX.Element) => {
          renders.push(render)
        },
      },
    },
  } as unknown as Partial<TuiPluginApi>)
  const store = {
    setupCommands: [{ name: "setup", cwd: ".", command: "bun install" }],
    configuredScopes: scopes,
    refresh: async () => {
      refreshes++
    },
    notify: (notice: Notice) => notices.push(notice),
  }
  return {
    api,
    store,
    notices,
    renders,
    get cleared() {
      return cleared
    },
    get refreshes() {
      return refreshes
    },
  }
}

async function renderLatest(api: TuiPluginApi, renders: Array<() => JSX.Element>) {
  await renderSetup?.renderer.destroy()
  renderSetup = await testRender(() => renders.at(-1)!(), { width: 80, height: 24 })
  attachRendererMockInput(api, renderSetup)
  await renderSetup.flush()
}

function lastKeyConsumed(api: TuiPluginApi): boolean {
  return !!(api.renderer.keyInput as { lastConsumed?: boolean }).lastConsumed
}

// Emit a raw key event through the mock keyInput. Used for chords the mockInput helper cannot
// name directly (e.g. the Kitty-mode "linefeed" name, or shift+return).
function emitKey(api: TuiPluginApi, event: Partial<KeyEvent> & { name: string }) {
  ;(api.renderer.keyInput as unknown as { emitKey: (key: KeyEvent) => void }).emitKey(event as KeyEvent)
}

async function focusField(tabCount: number) {
  for (let i = 0; i < tabCount; i++) {
    renderSetup!.mockInput.pressTab()
    await renderSetup!.flush()
  }
}

describe("openCreateTaskDialog", () => {
  test("warns and does not create when title is blank", async () => {
    const view = harness()
    await openCreateTaskDialog(view.api, view.store as never)
    await renderLatest(view.api, view.renders)

    renderSetup!.mockInput.pressEnter()
    await renderSetup!.flush()

    expect(createInput).toBeUndefined()
    expect(view.cleared).toBe(false)
    expect(view.notices).toEqual([{ variant: "warning", title: "Kagan", message: "Title is required" }])
  })

  test("creates with the chosen model, current branch, order append, refresh, and success notice", async () => {
    const view = harness()
    await openCreateTaskDialog(view.api, view.store as never)
    await renderLatest(view.api, view.renders)

    await renderSetup!.mockInput.typeText("  Ship docs  ")
    renderSetup!.mockInput.pressTab()
    await renderSetup!.flush()
    renderSetup!.mockInput.pressTab()
    await renderSetup!.flush()
    renderSetup!.mockInput.pressTab()
    await renderSetup!.flush()
    renderSetup!.mockInput.pressEnter()
    await renderLatest(view.api, view.renders)
    renderSetup!.mockInput.pressKey("ARROW_DOWN")
    await renderSetup!.flush()
    renderSetup!.mockInput.pressEnter()
    await renderLatest(view.api, view.renders)

    renderSetup!.mockInput.pressTab()
    await renderSetup!.flush()
    renderSetup!.mockInput.pressTab()
    await renderSetup!.flush()
    renderSetup!.mockInput.pressEnter()
    await renderSetup!.flush()

    expect(createInput).toEqual({
      title: "Ship docs",
      description: "",
      model: { providerID: "anthropic", modelID: "sonnet" },
      baseBranch: "feature",
      setupCommands: [{ name: "setup", cwd: ".", command: "bun install" }],
    })
    expect(writtenOrder).toEqual(["old", "new1"])
    expect(view.refreshes).toBe(1)
    expect(view.notices.at(-1)).toEqual({ variant: "success", title: "Kagan", message: 'Created "Ship docs"' })
  })

  test("enter submits from the description field", async () => {
    const view = harness()
    await openCreateTaskDialog(view.api, view.store as never)
    await renderLatest(view.api, view.renders)

    await renderSetup!.mockInput.typeText("Ship docs")
    await focusField(1)
    renderSetup!.mockInput.pressEnter()
    await renderSetup!.flush()

    expect(createInput).toMatchObject({ title: "Ship docs", baseBranch: "feature" })
  })

  test("ctrl+j inserts a newline in the focused description field and consumes the key instead of submitting", async () => {
    const newLine = spyOn(EditBufferRenderable.prototype, "newLine").mockImplementation(() => true)
    try {
      const view = harness()
      await openCreateTaskDialog(view.api, view.store as never)
      await renderLatest(view.api, view.renders)

      await renderSetup!.mockInput.typeText("Ship docs")
      await focusField(1)
      renderSetup!.mockInput.pressKey("j", { ctrl: true })
      await renderSetup!.flush()

      // The real test renderer also decodes legacy Ctrl+J as linefeed into the textarea, so newLine
      // may fire both from that path and from our intercept — assert it fired, not an exact count.
      expect(newLine).toHaveBeenCalled()
      expect(createInput).toBeUndefined()
      expect(lastKeyConsumed(view.api)).toBe(true)
    } finally {
      newLine.mockRestore()
    }
  })

  test("linefeed (legacy Ctrl+J decode) and shift+return also insert a newline in the description", async () => {
    const newLine = spyOn(EditBufferRenderable.prototype, "newLine").mockImplementation(() => true)
    try {
      const view = harness()
      await openCreateTaskDialog(view.api, view.store as never)
      await renderLatest(view.api, view.renders)

      await renderSetup!.mockInput.typeText("Ship docs")
      await focusField(1)

      emitKey(view.api, { name: "linefeed" })
      expect(lastKeyConsumed(view.api)).toBe(true)

      emitKey(view.api, { name: "return", shift: true })
      expect(lastKeyConsumed(view.api)).toBe(true)

      expect(newLine).toHaveBeenCalledTimes(2)
      expect(createInput).toBeUndefined()
    } finally {
      newLine.mockRestore()
    }
  })

  test("ctrl+j outside the description field is not treated as a newline", async () => {
    const newLine = spyOn(EditBufferRenderable.prototype, "newLine").mockImplementation(() => true)
    try {
      const view = harness()
      await openCreateTaskDialog(view.api, view.store as never)
      await renderLatest(view.api, view.renders)

      // focusIndex 0 (title): the newline handler must not fire.
      renderSetup!.mockInput.pressKey("j", { ctrl: true })
      await renderSetup!.flush()

      expect(newLine).not.toHaveBeenCalled()
      expect(lastKeyConsumed(view.api)).toBe(false)
    } finally {
      newLine.mockRestore()
    }
  })

  test("ctrl+enter still submits from the description field on Kitty-style terminals", async () => {
    const view = harness()
    await openCreateTaskDialog(view.api, view.store as never)
    await renderSetup?.renderer.destroy()
    renderSetup = await testRender(() => view.renders.at(-1)!(), { width: 80, height: 24, otherModifiersMode: true })
    attachRendererMockInput(view.api, renderSetup)
    await renderSetup.flush()

    await renderSetup.mockInput.typeText("Ship docs")
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter({ ctrl: true })
    await renderSetup.flush()

    expect(createInput).toMatchObject({ title: "Ship docs", baseBranch: "feature" })
  })

  test("return is intercepted from every focus field; ctrl+j is intercepted in the description", async () => {
    async function assertReturnConsumed(tabToFocus: number, scopes: string[] = []) {
      const view = harness(scopes)
      await openCreateTaskDialog(view.api, view.store as never)
      await renderLatest(view.api, view.renders)
      await focusField(tabToFocus)
      renderSetup!.mockInput.pressEnter()
      await renderSetup!.flush()
      expect(lastKeyConsumed(view.api)).toBe(true)
    }

    await assertReturnConsumed(0)
    await assertReturnConsumed(1)
    await assertReturnConsumed(2, ["alpha", "beta"])
    await assertReturnConsumed(3)
    await assertReturnConsumed(4)

    const newLine = spyOn(EditBufferRenderable.prototype, "newLine").mockImplementation(() => true)
    try {
      const view = harness()
      await openCreateTaskDialog(view.api, view.store as never)
      await renderLatest(view.api, view.renders)
      await focusField(1)
      renderSetup!.mockInput.pressKey("j", { ctrl: true })
      await renderSetup!.flush()
      expect(lastKeyConsumed(view.api)).toBe(true)
    } finally {
      newLine.mockRestore()
    }
  })

  test("preselects the only configured static scope", async () => {
    const view = harness(["project-alpha"])
    await openCreateTaskDialog(view.api, view.store as never)
    await renderLatest(view.api, view.renders)

    await renderSetup!.mockInput.typeText("Ship alpha project")
    renderSetup!.mockInput.pressEnter()
    await renderSetup!.flush()

    expect(createInput).toMatchObject({ title: "Ship alpha project", scope: { values: ["project-alpha"] } })
  })

  test("requires a scope when multiple static scopes are configured", async () => {
    const view = harness(["project-alpha", "project-beta"])
    await openCreateTaskDialog(view.api, view.store as never)
    await renderLatest(view.api, view.renders)

    await renderSetup!.mockInput.typeText("Ship scoped work")
    renderSetup!.mockInput.pressEnter()
    await renderSetup!.flush()

    expect(createInput).toBeUndefined()
    expect(view.cleared).toBe(false)
    expect(view.notices).toContainEqual({ variant: "warning", title: "Kagan", message: "Scope is required" })
  })

  test("reports create failures", async () => {
    createError = new Error("worktree failed")
    const view = harness()
    await openCreateTaskDialog(view.api, view.store as never)
    await renderLatest(view.api, view.renders)

    await renderSetup!.mockInput.typeText("Fix flaky test")
    renderSetup!.mockInput.pressEnter()
    await renderSetup!.flush()

    expect(writtenOrder).toBeUndefined()
    expect(view.refreshes).toBe(0)
    expect(view.notices.at(-1)).toEqual({ variant: "error", title: "Kagan", message: "worktree failed" })
  })
})
