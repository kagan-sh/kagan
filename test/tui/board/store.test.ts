import { describe, expect, spyOn, test } from "bun:test"
import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import type { Event, SessionStatus } from "@opencode-ai/sdk/v2"
import { createRoot } from "solid-js"
import {
  createBoardStore,
  createSessionEventSubscription,
  createSessionStatusSubscription,
  SESSION_EVENT_DEBOUNCE_MS,
} from "../../../src/tui/board/store"
import { ROUTE, type BoardSession } from "../../../src/tui/types"
import { COLUMNS, type ColumnType } from "../../../src/domain/task/types"
import { mockSession } from "../../fixtures/api"

function assertDefined<T>(value: T | undefined | null): asserts value is T {
  expect(value).toBeDefined()
}

function session(id: string, status: ColumnType, title: string, updated = 0, parentID?: string): BoardSession {
  return mockSession(id, status, title, updated, parentID)
}

const emptyOrders: Record<ColumnType, readonly string[]> = {
  backlog: [],
  in_progress: [],
  review: [],
  done: [],
}

function mockEventApi(routeName: string) {
  const handlers = new Map<Event["type"], Array<(event?: unknown) => void>>()
  const api = {
    route: { current: { name: routeName } },
    event: {
      on(type: Event["type"], handler: (event?: unknown) => void) {
        handlers.set(type, [...(handlers.get(type) ?? []), handler])
        return () => {
          handlers.set(type, handlers.get(type)?.filter((item) => item !== handler) ?? [])
        }
      },
    },
  } as TuiPluginApi
  return {
    api,
    trigger(type: Event["type"], event?: unknown) {
      handlers.get(type)?.forEach((handler) => handler(event))
    },
  }
}

describe("createSessionEventSubscription", () => {
  const DEBOUNCE_TEST_MS = 10

  function withDebounce<T>(run: () => Promise<T>): Promise<T> {
    const original = globalThis.setTimeout
    const timer = spyOn(globalThis, "setTimeout").mockImplementation(((fn: TimerHandler, ms?: number) =>
      original(fn, ms === SESSION_EVENT_DEBOUNCE_MS ? DEBOUNCE_TEST_MS : (ms ?? 0))) as typeof setTimeout)
    return run().finally(() => timer.mockRestore())
  }

  test("collapses a burst of session events into a single debounced refresh", async () => {
    await withDebounce(async () => {
      const { api, trigger } = mockEventApi(ROUTE)
      let calls = 0
      createSessionEventSubscription(api, async () => {
        calls++
      })
      trigger("session.created")
      trigger("session.updated")
      trigger("session.idle")
      trigger("session.deleted")
      expect(calls).toBe(0)
      await new Promise((resolve) => setTimeout(resolve, 30))
      expect(calls).toBe(1)
    })
  })

  test("restarts the debounce window on each new event instead of refreshing on a fixed cadence", async () => {
    await withDebounce(async () => {
      const { api, trigger } = mockEventApi(ROUTE)
      let calls = 0
      createSessionEventSubscription(api, async () => {
        calls++
      })
      trigger("session.created")
      await new Promise((resolve) => setTimeout(resolve, 6))
      trigger("session.updated")
      await new Promise((resolve) => setTimeout(resolve, 6))
      expect(calls).toBe(0)
      await new Promise((resolve) => setTimeout(resolve, 10))
      expect(calls).toBe(1)
    })
  })

  test("ignores session events while the route is inactive", async () => {
    await withDebounce(async () => {
      const { api, trigger } = mockEventApi("home")
      let calls = 0
      createSessionEventSubscription(api, async () => {
        calls++
      })
      trigger("session.created")
      await new Promise((resolve) => setTimeout(resolve, 30))
      expect(calls).toBe(0)
    })
  })

  test("disposing cancels a pending debounced refresh", async () => {
    await withDebounce(async () => {
      const { api, trigger } = mockEventApi(ROUTE)
      let calls = 0
      const dispose = createSessionEventSubscription(api, async () => {
        calls++
      })
      trigger("session.created")
      dispose()
      await new Promise((resolve) => setTimeout(resolve, 30))
      expect(calls).toBe(0)
    })
  })
})

describe("createSessionStatusSubscription", () => {
  test("reads sessionID/status off the session.status event's properties", () => {
    const { api, trigger } = mockEventApi(ROUTE)
    const statuses: Record<string, SessionStatus["type"]> = {}
    createSessionStatusSubscription(api, (sessionID, status) => {
      statuses[sessionID] = status
    })
    trigger("session.status", { properties: { sessionID: "s1", status: { type: "busy" } } })
    expect(statuses.s1).toBe("busy")
  })

  test("disposing stops further updates", () => {
    const { api, trigger } = mockEventApi(ROUTE)
    let calls = 0
    const dispose = createSessionStatusSubscription(api, () => calls++)
    dispose()
    trigger("session.status", { properties: { sessionID: "s1", status: { type: "busy" } } })
    expect(calls).toBe(0)
  })
})

function mockStoreApi(
  options: {
    sessions?: BoardSession[]
    orders?: Record<ColumnType, readonly string[]>
    kv?: Record<string, unknown>
    moveError?: Error
    deleteError?: Error
    list?: () => Promise<{ data: BoardSession[] }>
  } = {},
): TuiPluginApi & { kvMap: Record<string, unknown>; deleted: string[]; toasts: TuiToast[] } {
  const kvMap: Record<string, unknown> = { ...options.kv }
  const deleted: string[] = []
  const toasts: TuiToast[] = []
  const orders = options.orders ?? emptyOrders
  for (const column of COLUMNS) {
    kvMap[`kagan:order:${column}`] = [...orders[column]]
  }

  let sessionList = options.sessions ?? []

  return {
    kvMap,
    deleted,
    toasts,
    kv: {
      get: (key: string, defaultValue: unknown) => (key in kvMap ? kvMap[key] : defaultValue),
      set: (key: string, value: unknown) => {
        kvMap[key] = value
      },
    },
    client: {
      session: {
        list:
          options.list ??
          (async () => ({
            data: sessionList,
          })),
        get: async ({ sessionID }: { sessionID: string }) => ({
          data: sessionList.find((s) => s.id === sessionID) ?? { metadata: {} },
        }),
        update: async () => {
          if (options.moveError) throw options.moveError
          return { data: undefined }
        },
        abort: async () => ({ data: undefined }),
        children: async () => ({ data: [] }),
        delete: async ({ sessionID }: { sessionID: string }) => {
          if (options.deleteError) throw options.deleteError
          deleted.push(sessionID)
          sessionList = sessionList.filter((session) => session.id !== sessionID)
          return { data: undefined }
        },
      },
    },
    ui: {
      toast: (input: TuiToast) => {
        toasts.push(input)
      },
      dialog: { open: false },
    },
    route: { current: { name: ROUTE } },
    theme: { current: {} },
  } as unknown as TuiPluginApi & { kvMap: Record<string, unknown>; deleted: string[]; toasts: TuiToast[] }
}

const intakeReadyMetadata = { kagan: { status: "backlog", boardTask: true, intakeOutcome: "ran", worktree: "/wt" } }

describe("createBoardStore", () => {
  test("squashMerge defaults to true and reflects the resolved plugin option", () => {
    const api = mockStoreApi({ sessions: [], orders: emptyOrders })
    expect(createRoot(() => createBoardStore(api)).squashMerge).toBe(true)
    expect(createRoot(() => createBoardStore(api, { squashMerge: false })).squashMerge).toBe(false)
    expect(createRoot(() => createBoardStore(api, { squashMerge: true })).squashMerge).toBe(true)
  })

  test("checkCommand reflects configured check commands, treating blank as undefined", () => {
    const api = mockStoreApi({ sessions: [], orders: emptyOrders })
    expect(createRoot(() => createBoardStore(api)).checkCommand).toBeUndefined()
    expect(
      createRoot(
        () =>
          createBoardStore(api, { commands: { check: [{ name: "check", cwd: ".", command: "bun test" }] } })
            .checkCommand,
      ),
    ).toBe("bun test")
    expect(
      createRoot(
        () => createBoardStore(api, { commands: { check: [{ name: "check", cwd: ".", command: "" }] } }).checkCommand,
      ),
    ).toBeUndefined()
    expect(
      createRoot(
        () =>
          createBoardStore(api, { commands: { check: [{ name: "check", cwd: ".", command: "   " }] } }).checkCommand,
      ),
    ).toBeUndefined()
  })

  test("moveTo does not update orders when moveSession fails", async () => {
    const api = mockStoreApi({
      sessions: [{ ...session("s1", "backlog", "Task"), metadata: intakeReadyMetadata }],
      orders: { ...emptyOrders, backlog: ["s1"] },
      moveError: new Error("move failed"),
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.moveTo("in_progress")
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["s1"])
    expect(api.kvMap["kagan:order:in_progress"]).toEqual([])
  })

  test("moveTo updates selectedColumn to the target column", async () => {
    const api = mockStoreApi({
      sessions: [{ ...session("s1", "backlog", "Task"), metadata: intakeReadyMetadata }],
      orders: { ...emptyOrders, backlog: ["s1"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.moveTo("in_progress")
    expect(store.selectedColumn()).toBe("in_progress")
  })

  test("moveTo blocks moving into in_progress when intake decisions are still pending", async () => {
    const api = mockStoreApi({
      sessions: [
        {
          ...session("s1", "backlog", "Task"),
          metadata: {
            kagan: {
              status: "backlog",
              boardTask: true,
              intakeOutcome: "ran",
              worktree: "/wt",
              intake: {
                understanding: "Adds a retry wrapper.",
                decisions: [{ id: "d1", question: "Max retries?", assumption: "3", required: true }],
              },
            },
          },
        },
      ],
      orders: { ...emptyOrders, backlog: ["s1"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.moveTo("in_progress")
    expect(store.notices().at(-1)).toMatchObject({
      variant: "error",
      title: "Kagan",
      message: "1 intake decision(s) need your answer before starting",
    })
    expect(api.kvMap["kagan:order:in_progress"]).toEqual([])
  })

  test("moveTo blocks moving a subtask between columns", async () => {
    const api = mockStoreApi({
      sessions: [session("parent", "backlog", "Parent"), session("child", "backlog", "Child", 0, "parent")],
      orders: { ...emptyOrders, backlog: ["parent"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "child")
    await store.moveTo("in_progress")
    expect(store.notices().at(-1)).toMatchObject({
      variant: "error",
      title: "Kagan",
      message: "Subtasks cannot be moved between columns",
    })
  })

  test("moveTo blocks moving into in_progress at the WIP cap", async () => {
    const api = mockStoreApi({
      sessions: [
        { ...session("wip1", "in_progress", "WIP 1"), metadata: { kagan: { status: "in_progress", boardTask: true } } },
        { ...session("wip2", "in_progress", "WIP 2"), metadata: { kagan: { status: "in_progress", boardTask: true } } },
        session("s1", "backlog", "Task"),
      ],
      orders: {
        ...emptyOrders,
        backlog: ["s1"],
        in_progress: ["wip1", "wip2"],
      },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.moveTo("in_progress")
    expect(store.notices().at(-1)).toMatchObject({
      variant: "error",
      title: "Kagan",
      message: "In Progress WIP limit of 2 reached",
    })
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["s1"])
    expect(api.kvMap["kagan:order:in_progress"]).toEqual(["wip1", "wip2"])
  })

  test("moveTo blocks moving to done when not approved", async () => {
    const api = mockStoreApi({
      sessions: [session("s1", "review", "Task")],
      orders: { ...emptyOrders, review: ["s1"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("review", "s1")
    await store.moveTo("done")
    const toast = store.notices().at(-1)
    expect(toast?.variant === "error" && toast.message.includes("approved")).toBe(true)
    expect(api.kvMap["kagan:order:done"]).toEqual([])
  })

  test("moveTo does not block before moveSession when gate allows but update fails", async () => {
    const api = mockStoreApi({
      sessions: [{ ...session("s1", "backlog", "Task"), metadata: intakeReadyMetadata }],
      orders: { ...emptyOrders, backlog: ["s1"] },
      moveError: new Error("move failed"),
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.moveTo("in_progress")
    expect(store.notices().at(-1)).toMatchObject({ variant: "error", title: "Kagan", message: "move failed" })
  })

  test("notify stacks multiple notices instead of the latest replacing the others", () => {
    const api = mockStoreApi({ sessions: [] })
    const store = createRoot(() => createBoardStore(api))
    store.notify({ variant: "error", title: "Kagan", message: "intake failed" })
    store.notify({ variant: "warning", title: "Kagan", message: "waiting on you" })
    expect(store.notices().map((notice) => notice.message)).toEqual(["intake failed", "waiting on you"])
  })

  test("notify drops the oldest notice once a fourth notice arrives", () => {
    const api = mockStoreApi({ sessions: [] })
    const store = createRoot(() => createBoardStore(api))
    store.notify({ variant: "info", title: "Kagan", message: "one" })
    store.notify({ variant: "info", title: "Kagan", message: "two" })
    store.notify({ variant: "info", title: "Kagan", message: "three" })
    store.notify({ variant: "info", title: "Kagan", message: "four" })
    expect(store.notices().map((notice) => notice.message)).toEqual(["two", "three", "four"])
  })

  test("notify gives the error variant a longer duration than other variants", () => {
    const durations: number[] = []
    const original = globalThis.setTimeout
    const timer = spyOn(globalThis, "setTimeout").mockImplementation(((fn: TimerHandler, ms?: number) => {
      if (typeof ms === "number") durations.push(ms)
      return original(fn, ms ?? 0)
    }) as typeof setTimeout)
    try {
      const api = mockStoreApi({ sessions: [] })
      const store = createRoot(() => createBoardStore(api))
      store.notify({ variant: "error", title: "Kagan", message: "boom" })
      store.notify({ variant: "warning", title: "Kagan", message: "careful" })
      expect(durations[0]).toBe(10000)
      expect(durations[1]).toBe(5000)
    } finally {
      timer.mockRestore()
    }
  })

  test("refresh re-reads the persisted filter from KV", async () => {
    const api = mockStoreApi({ sessions: [] })
    const store = createRoot(() => createBoardStore(api))
    expect(store.filter()).toBe("")
    api.kvMap["kagan:filter"] = "board"
    await store.refresh()
    expect(store.filter()).toBe("board")
  })

  test("select resets to the first visible card when the current selection is filtered out", async () => {
    const api = mockStoreApi({
      sessions: [session("s1", "backlog", "Apple"), session("s2", "backlog", "Banana")],
      orders: { ...emptyOrders, backlog: ["s1", "s2"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s2")
    store.setFilter("app")
    expect(store.selected()).toBe("s1")
    expect(store.selectedColumn()).toBe("backlog")
  })

  test("#N filter matches the card whose task number equals N exactly", async () => {
    const api = mockStoreApi({
      sessions: [
        mockSession("s1", "backlog", "Task 3", 0, undefined, {
          metadata: { kagan: { status: "backlog", boardTask: true, taskNumber: 3 } },
        }),
        mockSession("s2", "backlog", "Task 30", 0, undefined, {
          metadata: { kagan: { status: "backlog", boardTask: true, taskNumber: 30 } },
        }),
      ],
      orders: { ...emptyOrders, backlog: ["s1", "s2"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.setFilter("#3")
    expect(store.selected()).toBe("s1")
    expect(store.columns().backlog.map((card) => card.session.id)).toEqual(["s1"])
  })

  test("refresh surfaces a warning notice the first time a task's helperError appears, not on every subsequent poll", async () => {
    const healthy = session("s1", "review", "Task")
    const failed = {
      ...healthy,
      metadata: { kagan: { status: "review", boardTask: true, helperError: { role: "validator", message: "boom" } } },
    }
    let call = 0
    const api = mockStoreApi({
      list: async () => ({ data: call++ === 0 ? [healthy] : [failed] }),
    })
    const store = createRoot(() => createBoardStore(api))

    await store.refresh()
    expect(store.notices()).toEqual([])

    await store.refresh()
    expect(store.notices()).toHaveLength(1)
    expect(store.notices()[0]).toMatchObject({
      variant: "warning",
      title: "Kagan",
      message: "Review failed for s1 — boom — press r to retry",
    })

    store.notify({ variant: "success", title: "Kagan", message: "cleared" })
    await store.refresh()
    expect(store.notices()).toHaveLength(2)
    expect(store.notices().at(-1)).toMatchObject({ variant: "success", title: "Kagan", message: "cleared" })
  })

  test("refresh surfaces a warning notice the first time a task starts awaiting a permission reply", async () => {
    const healthy = session("s1", "in_progress", "Task")
    const waiting = {
      ...healthy,
      metadata: {
        kagan: {
          status: "in_progress",
          boardTask: true,
          awaitingPermissions: [{ id: "p1", title: "Run rm -rf?", sessionID: "s1" }],
        },
      },
    }
    let call = 0
    const api = mockStoreApi({
      list: async () => ({ data: call++ === 0 ? [healthy] : [waiting] }),
    })
    const store = createRoot(() => createBoardStore(api))

    await store.refresh()
    expect(store.notices()).toEqual([])

    await store.refresh()
    expect(store.notices()[0]).toMatchObject({
      variant: "warning",
      title: "Kagan",
      message: "s1 waiting on you — Run rm -rf? — press p",
    })
  })

  test("refresh ignores stale listSessions results", async () => {
    const resolvers: Array<(sessions: BoardSession[]) => void> = []
    const api = mockStoreApi({
      list: () =>
        new Promise<{ data: BoardSession[] }>((resolve) => {
          resolvers.push((sessions) => resolve({ data: sessions }))
        }),
    })
    const store = createRoot(() => createBoardStore(api))

    const first = store.refresh()
    const second = store.refresh()
    assertDefined(resolvers[0])
    assertDefined(resolvers[1])

    resolvers[1]([session("new", "backlog", "New")])
    await second
    expect(store.sessions().map((item) => item.id)).toEqual(["new"])

    resolvers[0]([session("old", "backlog", "Old")])
    await first
    expect(store.sessions().map((item) => item.id)).toEqual(["new"])
  })

  test("deleteSelected removes the session and selects the next card", async () => {
    const api = mockStoreApi({
      sessions: [session("s1", "backlog", "First"), session("s2", "backlog", "Second")],
      orders: { ...emptyOrders, backlog: ["s1", "s2"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.deleteSelected()
    expect(api.deleted).toEqual(["s1"])
    expect(store.sessions().map((item) => item.id)).toEqual(["s2"])
    expect(store.selected()).toBe("s2")
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["s2"])
  })

  test("deleteSelected selects the first card of another column when the current column becomes empty", async () => {
    const api = mockStoreApi({
      sessions: [session("s1", "backlog", "Only backlog"), session("s2", "review", "Review task")],
      orders: { ...emptyOrders, backlog: ["s1"], review: ["s2"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.deleteSelected()
    expect(api.deleted).toEqual(["s1"])
    expect(store.selected()).toBe("s2")
    expect(store.selectedColumn()).toBe("review")
  })

  test("deleteSelected clears selection once no sessions remain in any column", async () => {
    const api = mockStoreApi({
      sessions: [session("s1", "backlog", "Only task")],
      orders: { ...emptyOrders, backlog: ["s1"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.deleteSelected()
    expect(store.selected()).toBeUndefined()
  })

  test("deleteSelected leaves selection and orders untouched when the delete call fails", async () => {
    const api = mockStoreApi({
      sessions: [session("s1", "backlog", "First"), session("s2", "backlog", "Second")],
      orders: { ...emptyOrders, backlog: ["s1", "s2"] },
      deleteError: new Error("delete failed"),
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "s1")
    await store.deleteSelected()
    expect(api.deleted).toEqual([])
    expect(store.selected()).toBe("s1")
    expect(store.selectedColumn()).toBe("backlog")
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["s1", "s2"])
  })

  test("in review, an unapproved card sorts above an approved one after refresh", async () => {
    const approved = mockSession("s1", "review", "Approved", 0, undefined, {
      metadata: { kagan: { status: "review", boardTask: true, approved: true } },
    })
    const unapproved = mockSession("s2", "review", "Unapproved", 0, undefined, {
      metadata: { kagan: { status: "review", boardTask: true } },
    })
    const api = mockStoreApi({
      sessions: [approved, unapproved],
      orders: { ...emptyOrders, review: ["s1", "s2"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    expect(store.columns().review.map((card) => card.session.id)).toEqual(["s2", "s1"])
  })

  test("refresh reconciles orders: drops a stale id, relocates a reclassified id, and appends a new root", async () => {
    const moved = mockSession("moved", "review", "Moved", 0, undefined, {
      metadata: { kagan: { status: "review", boardTask: true } },
    })
    const fresh = session("new", "backlog", "New")
    const api = mockStoreApi({
      sessions: [moved, fresh],
      orders: { ...emptyOrders, backlog: ["stale", "moved"], review: [] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["new"])
    expect(api.kvMap["kagan:order:review"]).toEqual(["moved"])
  })

  test("selectNext navigates from parent into subtasks", async () => {
    const api = mockStoreApi({
      sessions: [session("parent", "backlog", "Parent", 2), session("child", "backlog", "Child", 3, "parent")],
      orders: { ...emptyOrders, backlog: ["parent"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "parent")
    store.selectNext()
    expect(store.selected()).toBe("child")
    expect(store.selectedColumn()).toBe("backlog")
  })

  test("reorder swaps the selected card down and persists the order", async () => {
    const api = mockStoreApi({
      sessions: [session("a", "backlog", "A"), session("b", "backlog", "B"), session("c", "backlog", "C")],
      orders: { ...emptyOrders, backlog: ["a", "b", "c"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "a")
    store.reorder(1)
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["b", "a", "c"])
    expect(store.columns().backlog.map((card) => card.session.id)).toEqual(["b", "a", "c"])
  })

  test("reorder swaps the selected card up and persists the order", async () => {
    const api = mockStoreApi({
      sessions: [session("a", "backlog", "A"), session("b", "backlog", "B")],
      orders: { ...emptyOrders, backlog: ["a", "b"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "b")
    store.reorder(-1)
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["b", "a"])
  })

  test("reorder up on the top card is a no-op", async () => {
    const api = mockStoreApi({
      sessions: [session("a", "backlog", "A"), session("b", "backlog", "B")],
      orders: { ...emptyOrders, backlog: ["a", "b"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "a")
    store.reorder(-1)
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["a", "b"])
  })

  test("reorder down on the bottom card is a no-op", async () => {
    const api = mockStoreApi({
      sessions: [session("a", "backlog", "A"), session("b", "backlog", "B")],
      orders: { ...emptyOrders, backlog: ["a", "b"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "b")
    store.reorder(1)
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["a", "b"])
  })

  test("reorder is a no-op when a subtask is selected", async () => {
    const api = mockStoreApi({
      sessions: [session("parent", "backlog", "Parent"), session("child", "backlog", "Child", 0, "parent")],
      orders: { ...emptyOrders, backlog: ["parent"] },
    })
    const store = createRoot(() => createBoardStore(api))
    await store.refresh()
    store.select("backlog", "parent")
    store.selectNext()
    expect(store.selected()).toBe("child")
    store.reorder(1)
    expect(api.kvMap["kagan:order:backlog"]).toEqual(["parent"])
  })
})
