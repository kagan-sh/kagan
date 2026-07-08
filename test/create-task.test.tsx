/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, mock, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { JSX } from "solid-js"
import { attachRendererMockInput, mockTuiApi } from "./fixtures/api"

let branches = ["main", "feature"]
let createInput: Record<string, unknown> | undefined
let createError: Error | undefined
let backlogOrder = ["old"]
let writtenOrder: string[] | undefined

mock.module("../src/git", () => ({
  bunGitRunner: () => async () => ({ code: 0, stdout: "", stderr: "" }),
  listLocalBranches: async () => branches,
}))

mock.module("../src/session-api", () => ({
  createTask: async (_api: TuiPluginApi, input: Record<string, unknown>) => {
    if (createError) throw createError
    createInput = input
    return { id: "new1" }
  },
  getOrder: () => backlogOrder,
  setOrder: (_api: TuiPluginApi, _column: string, order: string[]) => {
    writtenOrder = order
  },
}))

const { openCreateTaskDialog } = await import("../src/create-task")

type Notice = { variant: string; title: string; message: string }

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
  branches = ["main", "feature"]
  createInput = undefined
  createError = undefined
  backlogOrder = ["old"]
  writtenOrder = undefined
})

function harness(scopes: string[] = []) {
  const notices: Notice[] = []
  const renders: Array<() => JSX.Element> = []
  let cleared = false
  let refreshes = 0
  let selectProps: { title: string; onSelect: (option: { value: number }) => void } | undefined
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
      DialogSelect: (props: { title: string; onSelect: (option: { value: number }) => void }) => {
        selectProps = props
        return (
          <box>
            <text>{props.title}</text>
          </box>
        )
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
    get selectProps() {
      return selectProps
    },
  }
}

async function renderLatest(api: TuiPluginApi, renders: Array<() => JSX.Element>) {
  await renderSetup?.renderer.destroy()
  renderSetup = await testRender(() => renders.at(-1)!(), { width: 80, height: 24 })
  attachRendererMockInput(api, renderSetup)
  await renderSetup.flush()
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
    expect(view.selectProps?.title).toBe("Model")
    view.selectProps?.onSelect({ value: 1 })
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

  test("ctrl+enter submits from the description field instead of inserting a newline", async () => {
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
