/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import { createRoot } from "solid-js"
import type { TuiToast } from "@opencode-ai/plugin/tui"
import { Board } from "../src/board"
import { createBoardStore } from "../src/store"
import { ROUTE, type BoardSession, type ColumnType } from "../src/types"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { mockSession, mockTuiApi } from "./fixtures/api"
import type { TestRendererSetup } from "@opentui/core/testing"

let lastLayer: Parameters<TuiPluginApi["keymap"]["registerLayer"]>[0] | undefined

function keyName(key: string | { name?: string }): string {
  return typeof key === "string" ? key : (key.name ?? "")
}

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
  lastLayer = undefined
})

function session(id: string, status: ColumnType, title: string): BoardSession {
  return mockSession(id, status, title)
}

function mockBoardApi(
  options: {
    sessions?: BoardSession[]
    filter?: string
    dialogOpen?: boolean
    routeName?: string
    onboardingSeen?: boolean
  } = {},
): TuiPluginApi & { toasts: TuiToast[]; dialogReplaces: number } {
  const kvMap: Record<string, unknown> = { "kagan:onboarding": options.onboardingSeen ?? true }
  if (options.filter !== undefined) {
    kvMap["kagan:filter"] = options.filter
  }
  const toasts: TuiToast[] = []
  const api = {
    ...mockTuiApi({ kvMap }),
    client: {
      session: {
        list: async () => ({ data: options.sessions ?? [] }),
      },
    },
    ui: {
      toast: (input: TuiToast) => {
        toasts.push(input)
      },
      dialog: {
        open: options.dialogOpen ?? false,
        setSize: () => {},
        clear: () => {},
        replace: () => {
          api.dialogReplaces++
        },
      },
    },
    route: { current: { name: options.routeName ?? ROUTE } },
    keymap: {
      registerLayer: (layer: Parameters<TuiPluginApi["keymap"]["registerLayer"]>[0]) => {
        lastLayer = layer
        return () => {}
      },
    },
    toasts,
    dialogReplaces: 0,
  } as unknown as TuiPluginApi & { toasts: TuiToast[]; dialogReplaces: number }
  return api
}

describe("Board", () => {
  test("renders all four column headers without a brand header, separated by light rules", async () => {
    const api = mockBoardApi()
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    for (const label of ["Backlog", "In Progress", "Review", "Done"]) {
      expect(frame).toContain(label)
    }
    const headerLine = frame.split("\n").find((line) => line.includes("Backlog"))
    expect(headerLine?.split("│")).toHaveLength(4)
    expect(frame).not.toContain("Kagan")
  })

  test("dispatches board commands through the registered keymap layer", async () => {
    const api = mockBoardApi({
      sessions: [session("s1", "backlog", "First"), session("s2", "backlog", "Second")],
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()

    const commands = new Map(lastLayer?.commands?.map((command) => [command.name, command]))

    commands.get("kagan.down")?.run(undefined as never)
    await renderSetup.flush()
    expect(store.selected()).toBe("s2")

    commands.get("kagan.menu")?.run(undefined as never)
    await renderSetup.flush()
    expect(api.dialogReplaces).toBe(1)

    commands.get("kagan.help")?.run(undefined as never)
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("Help")

    const settingsBinding = lastLayer?.bindings?.find((binding) => binding.cmd === "kagan.settings")
    expect(settingsBinding).toBeTruthy()
    expect(keyName(settingsBinding!.key)).toContain(",")

    const filterBinding = lastLayer?.bindings?.find((binding) => binding.cmd === "kagan.filter")
    expect(filterBinding).toBeTruthy()
    expect(keyName(filterBinding!.key)).toContain("/")
  })

  test("renders the footer's baseline hints when nothing is selected", async () => {
    const api = mockBoardApi()
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("n new")
    expect(frame).toContain("? help")
    expect(frame).not.toContain("approve")
    expect(frame).not.toContain("d delete")
  })

  test("keeps the footer trimmed once a card is selected", async () => {
    const api = mockBoardApi({
      sessions: [
        { ...session("s1", "backlog", "Backlog task"), metadata: { kagan: { status: "backlog", boardTask: true } } },
      ],
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("enter menu")
    expect(frame).not.toContain("d delete")
    expect(frame).not.toContain("m move card")
  })

  test("a task created after mount appears on the next refresh", async () => {
    const sessions = [
      { ...session("s1", "backlog", "first task"), metadata: { kagan: { status: "backlog", boardTask: true } } },
    ]
    const api = mockBoardApi({ sessions })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("first task")

    sessions.push({
      ...session("s2", "backlog", "brand new task"),
      metadata: { kagan: { status: "backlog", boardTask: true } },
    })
    await store.refresh()
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("brand new task")
  })

  test("renders cards in their columns and nests child sessions", async () => {
    const api = mockBoardApi({
      sessions: [
        { ...session("s1", "backlog", "Backlog task"), metadata: { kagan: { status: "backlog", boardTask: true } } },
        {
          ...session("s2", "in_progress", "WIP task"),
          metadata: { kagan: { status: "in_progress", boardTask: true } },
        },
        {
          ...session("s3", "backlog", "Review i18n a11y"),
          parentID: "s1",
          metadata: { kagan: { status: "backlog", boardTask: true } },
        },
      ],
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 140, height: 24 })
    await renderSetup.waitForVisualIdle()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Backlog task")
    expect(frame).toContain("WIP task")
    expect(frame).toContain("1 subtask")
    expect(frame).toContain("i18n a11y")
    expect(frame).toMatch(/Backlog\s+1\b/)
    expect(frame).toMatch(/In Progress\s+1\/2/)
  })

  test("shows a live working indicator on an In Progress card once its session status arrives", async () => {
    const api = mockBoardApi({
      sessions: [
        {
          ...session("s1", "in_progress", "WIP task"),
          metadata: { kagan: { status: "in_progress", boardTask: true } },
        },
      ],
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).not.toContain("working")

    store.setSessionStatus("s1", "busy")
    await renderSetup.waitForFrame((frame) => frame.includes("working"))

    store.setSessionStatus("s1", "retry")
    await renderSetup.waitForFrame((frame) => frame.includes("retrying"))
    expect(renderSetup.captureCharFrame()).not.toContain("working")
  })

  test("keeps the working indicator off Backlog, Review, and Done cards", async () => {
    const api = mockBoardApi({
      sessions: [
        { ...session("s1", "backlog", "Backlog task"), metadata: { kagan: { status: "backlog", boardTask: true } } },
      ],
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    store.setSessionStatus("s1", "busy")
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).not.toContain("working")
  })

  test("shows the active filter", async () => {
    const api = mockBoardApi({ filter: "wip" })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("filter: wip")
  })

  test("keeps board bindings enabled while the help overlay is open", async () => {
    const api = mockBoardApi()
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    expect(lastLayer?.commands?.some((command) => command.name === "kagan.help")).toBe(true)
  })

  test("opens the onboarding dialog on first mount", async () => {
    const api = mockBoardApi({ onboardingSeen: false })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    expect(api.dialogReplaces).toBe(1)
  })

  test("store.notify drives the board-owned notice stack, never api.ui.toast (plugin routes render no toast)", async () => {
    const api = mockBoardApi()
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    expect(store.notices()).toEqual([])
    store.notify({ variant: "warning", title: "Kagan", message: "Intake is still being prepared" })
    expect(store.notices().at(-1)).toMatchObject({
      variant: "warning",
      title: "Kagan",
      message: "Intake is still being prepared",
    })
    expect(api.toasts).toHaveLength(0)
  })

  test("stacks two simultaneous notices instead of one overwriting the other", async () => {
    const api = mockBoardApi()
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    store.notify({ variant: "error", title: "Kagan", message: "Intake failed for #1" })
    store.notify({ variant: "warning", title: "Kagan", message: "#2 waiting on you" })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Intake failed for #1")
    expect(frame).toContain("#2 waiting on you")
  })

  test("keeps the selected card visible when navigating past a column's viewport", async () => {
    const sessions = Array.from({ length: 30 }, (_, i) => session(`s${i}`, "backlog", `Card ${i}`))
    const api = mockBoardApi({ sessions })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s0")
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 80, height: 24 })
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("Card 0")

    for (let i = 0; i < 29; i++) store.selectNext()
    await renderSetup.waitForFrame((frame) => frame.includes("Card 29"))

    for (let i = 0; i < 29; i++) store.selectPrevious()
    await renderSetup.waitForFrame((frame) => frame.includes("Card 0"))
  })

  test("scrolls horizontally so the selected column stays visible on a narrow terminal", async () => {
    const api = mockBoardApi({
      sessions: [
        session("s1", "backlog", "Backlog task"),
        session("s2", "in_progress", "WIP task"),
        session("s3", "review", "Review task"),
        session("s4", "done", "Done task"),
      ],
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 40, height: 24 })
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("Backlog task")

    store.selectNextColumn()
    store.selectNextColumn()
    store.selectNextColumn()
    await renderSetup.waitForFrame((frame) => frame.includes("Done task"))
  })

  test("renders the inline help overlay when kagan.help is triggered and hides it on second toggle", async () => {
    const api = mockBoardApi()
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    renderSetup = await testRender(() => <Board api={api} store={store} />, { width: 120, height: 20 })
    await renderSetup.flush()
    const help = lastLayer?.commands?.find((command) => command.name === "kagan.help")
    expect(help).toBeDefined()
    help?.run(undefined as never)
    await renderSetup.waitForVisualIdle()
    expect(renderSetup.captureCharFrame()).toContain("Help")
    help?.run(undefined as never)
    await renderSetup.waitForVisualIdle()
    expect(renderSetup.captureCharFrame()).not.toContain("Help")
  })
})
