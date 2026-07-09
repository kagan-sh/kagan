import { describe, expect, spyOn, test } from "bun:test"
import type { TuiPluginApi, TuiToast } from "@opencode-ai/plugin/tui"
import type { Event, SessionStatus } from "@opencode-ai/sdk/v2"
import { createRoot } from "solid-js"
import {
  adjacentColumn,
  attachChildren,
  createBoardStore,
  createSessionEventSubscription,
  createSessionStatusSubscription,
  detectNewAwaitingInput,
  detectNewHelperFailures,
  filterSessions,
  firstSessionID,
  flatNavIDs,
  groupCardsByColumn,
  nextSessionID,
  noticeDuration,
  reconcileOrders,
  rootSessions,
  SESSION_EVENT_DEBOUNCE_MS,
  sortNeedsHumanFirst,
  sortSessionsByOrder,
} from "../src/store"
import { COLUMNS, ROUTE, type BoardSession, type ColumnType } from "../src/types"
import { mockSession } from "./fixtures/api"

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

describe("filterSessions", () => {
  const sessions = [
    session("s1", "backlog", "Fix login"),
    session("s2", "in_progress", "Add board"),
    session("s3", "done", "Cleanup"),
  ]

  test("returns all sessions when filter is empty", () => {
    expect(filterSessions(sessions, "")).toHaveLength(3)
  })

  test("filters by title substring", () => {
    const result = filterSessions(sessions, "board")
    expect(result).toHaveLength(1)
    const first = result[0]
    assertDefined(first)
    expect(first.id).toBe("s2")
  })

  test("filters by slug substring", () => {
    const result = filterSessions(sessions, "s1")
    expect(result).toHaveLength(1)
    const first = result[0]
    assertDefined(first)
    expect(first.id).toBe("s1")
  })

  test("includes parent when a child matches", () => {
    const nested = [session("parent", "backlog", "Main task"), session("child", "backlog", "Review i18n", 0, "parent")]
    const result = filterSessions(nested, "i18n")
    expect(result.map((item) => item.id).sort()).toEqual(["child", "parent"])
  })

  test("plain-text queries are unaffected by the #N special case", () => {
    expect(filterSessions(sessions, "board").map((item) => item.id)).toEqual(["s2"])
  })
})

describe("filterSessions — #N task number", () => {
  function withTaskNumber(id: string, taskNumber: number): BoardSession {
    return mockSession(id, "backlog", `Task ${taskNumber}`, 0, undefined, {
      metadata: { kagan: { status: "backlog", boardTask: true, taskNumber } },
    })
  }

  test("#N matches the card whose task number equals N exactly", () => {
    const sessions = [withTaskNumber("s1", 3), withTaskNumber("s2", 31)]
    expect(filterSessions(sessions, "#3").map((item) => item.id)).toEqual(["s1"])
  })

  test("#N does not partially match a longer task number", () => {
    const sessions = [withTaskNumber("s1", 31)]
    expect(filterSessions(sessions, "#3")).toEqual([])
  })
})

describe("groupCardsByColumn", () => {
  test("groups sessions by kaganStatus and sorts by updated desc", () => {
    const sessions = [
      session("old", "backlog", "Old", 1),
      session("new", "backlog", "New", 2),
      session("progress", "in_progress", "Progress", 3),
    ]
    const grouped = groupCardsByColumn(sessions, emptyOrders)
    expect(grouped.backlog.map((card) => card.session)).toHaveLength(2)
    const first = grouped.backlog[0]?.session
    const second = grouped.backlog[1]?.session
    assertDefined(first)
    assertDefined(second)
    expect(first.id).toBe("new")
    expect(second.id).toBe("old")
    expect(grouped.in_progress).toHaveLength(1)
    expect(grouped.done).toHaveLength(0)
    expect(grouped.review).toHaveLength(0)
  })

  test("orders sessions by provided column order", () => {
    const sessions = [session("a", "backlog", "A", 3), session("b", "backlog", "B", 2), session("c", "backlog", "C", 1)]
    const grouped = groupCardsByColumn(sessions, {
      ...emptyOrders,
      backlog: ["c", "a"],
    })
    expect(grouped.backlog.map((card) => card.session.id)).toEqual(["c", "a", "b"])
  })

  test("excludes child sessions from top-level columns", () => {
    const sessions = [
      session("parent", "backlog", "Parent", 2),
      session("child", "backlog", "Child subtask", 3, "parent"),
    ]
    const grouped = groupCardsByColumn(sessions, emptyOrders)
    expect(grouped.backlog.map((card) => card.session.id)).toEqual(["parent"])
  })

  test("nests children under their parent card", () => {
    const sessions = [
      session("parent", "backlog", "Parent", 2),
      session("child-b", "backlog", "Older child", 1, "parent"),
      session("child-a", "backlog", "Newer child", 3, "parent"),
    ]
    const grouped = groupCardsByColumn(sessions, emptyOrders)
    expect(grouped.backlog).toHaveLength(1)
    const card = grouped.backlog[0]
    assertDefined(card)
    expect(card.session.id).toBe("parent")
    expect(card.children.map((item) => item.id)).toEqual(["child-a", "child-b"])
  })

  test("excludes non-board root sessions while keeping board tasks and their children", () => {
    const sessions = [
      session("task", "backlog", "Board task"),
      session("helper", "backlog", "task prep", 1, "task"),
      { ...session("chat", "backlog", "Generic chat session"), metadata: {} },
    ]
    const grouped = groupCardsByColumn(sessions, emptyOrders)
    expect(grouped.backlog.map((card) => card.session.id)).toEqual(["task"])
    expect(grouped.backlog[0]?.children.map((child) => child.id)).toEqual(["helper"])
  })

  test("floats an un-approved review card above an approved one in the review column", () => {
    const sessions = [
      {
        ...session("approved", "review", "Approved", 2),
        metadata: { kagan: { status: "review", boardTask: true, approved: true } },
      },
      { ...session("needs-you", "review", "Needs you", 1), metadata: { kagan: { status: "review", boardTask: true } } },
    ]
    const grouped = groupCardsByColumn(sessions, emptyOrders)
    expect(grouped.review.map((card) => card.session.id)).toEqual(["needs-you", "approved"])
  })

  test("does not reorder backlog or done columns by approval state", () => {
    const sessions = [session("a", "done", "A", 1), session("b", "done", "B", 2)]
    const grouped = groupCardsByColumn(sessions, emptyOrders)
    expect(grouped.done.map((card) => card.session.id)).toEqual(["b", "a"])
  })
})

describe("rootSessions", () => {
  test("returns only sessions without parentID", () => {
    const sessions = [session("parent", "backlog", "Parent"), session("child", "backlog", "Child", 0, "parent")]
    expect(rootSessions(sessions).map((item) => item.id)).toEqual(["parent"])
  })

  test("excludes archived sessions — the project-scoped list route does not filter them out itself", () => {
    const archived = mockSession("archived", "done", "Archived", 0, undefined, {
      time: { created: 0, updated: 0, archived: 123 },
    })
    const sessions = [session("live", "done", "Live"), archived]
    expect(rootSessions(sessions).map((item) => item.id)).toEqual(["live"])
  })
})

describe("attachChildren", () => {
  test("sorts children by updated desc", () => {
    const roots = [session("parent", "backlog", "Parent")]
    const sessions = [
      session("parent", "backlog", "Parent", 2),
      session("old", "backlog", "Old", 1, "parent"),
      session("new", "backlog", "New", 3, "parent"),
    ]
    const cards = attachChildren(roots, sessions)
    expect(cards[0]?.children.map((item) => item.id)).toEqual(["new", "old"])
  })
})

describe("sortSessionsByOrder", () => {
  test("orders sessions by the provided order array", () => {
    const sessions = [session("a", "backlog", "A", 1), session("b", "backlog", "B", 2), session("c", "backlog", "C", 3)]
    const result = sortSessionsByOrder(sessions, ["c", "a"])
    expect(result.map((item) => item.id)).toEqual(["c", "a", "b"])
  })

  test("falls back to updated desc for sessions not in the order array", () => {
    const sessions = [session("old", "backlog", "Old", 1), session("new", "backlog", "New", 2)]
    const result = sortSessionsByOrder(sessions, [])
    expect(result.map((item) => item.id)).toEqual(["new", "old"])
  })

  test("preserves only sessions that exist in the input", () => {
    const sessions = [session("a", "backlog", "A", 1)]
    const result = sortSessionsByOrder(sessions, ["a", "b"])
    expect(result.map((item) => item.id)).toEqual(["a"])
  })
})

describe("sortNeedsHumanFirst", () => {
  test("hoists un-approved review cards above approved ones, stably preserving relative order", () => {
    const sessions = [
      { ...session("approved-1", "review", "Approved 1"), metadata: { kagan: { status: "review", approved: true } } },
      { ...session("needs-1", "review", "Needs 1"), metadata: { kagan: { status: "review" } } },
      { ...session("approved-2", "review", "Approved 2"), metadata: { kagan: { status: "review", approved: true } } },
      { ...session("needs-2", "review", "Needs 2"), metadata: { kagan: { status: "review" } } },
    ]
    const result = sortNeedsHumanFirst(sessions)
    expect(result.map((item) => item.id)).toEqual(["needs-1", "needs-2", "approved-1", "approved-2"])
  })

  test("leaves non-review columns untouched regardless of approval state", () => {
    const sessions = [
      { ...session("a", "in_progress", "A"), metadata: { kagan: { status: "in_progress" } } },
      { ...session("b", "in_progress", "B"), metadata: { kagan: { status: "in_progress", approved: true } } },
    ]
    expect(sortNeedsHumanFirst(sessions).map((item) => item.id)).toEqual(["a", "b"])
  })
})

describe("adjacentColumn", () => {
  test("returns the next column", () => {
    expect(adjacentColumn("backlog", 1)).toBe("in_progress")
    expect(adjacentColumn("review", 1)).toBe("done")
  })

  test("returns undefined at the end", () => {
    expect(adjacentColumn("done", 1)).toBeUndefined()
  })

  test("returns the previous column", () => {
    expect(adjacentColumn("done", -1)).toBe("review")
  })

  test("returns undefined at the start", () => {
    expect(adjacentColumn("backlog", -1)).toBeUndefined()
  })
})

describe("flatNavIDs", () => {
  test("interleaves children after their parent", () => {
    const cards = attachChildren(
      [session("parent", "backlog", "Parent")],
      [
        session("parent", "backlog", "Parent"),
        session("child-b", "backlog", "Child B", 1, "parent"),
        session("child-a", "backlog", "Child A", 2, "parent"),
      ],
    )
    expect(flatNavIDs(cards)).toEqual(["parent", "child-a", "child-b"])
  })
})

describe("firstSessionID", () => {
  const columns = groupCardsByColumn([session("a", "backlog", "A"), session("b", "backlog", "B")], emptyOrders)

  test("returns the first session id in a column", () => {
    expect(firstSessionID(columns, "backlog")).toBe("a")
  })

  test("returns undefined for an empty column", () => {
    expect(firstSessionID(columns, "done")).toBeUndefined()
  })
})

describe("nextSessionID", () => {
  const columns = groupCardsByColumn(
    [session("a", "backlog", "A"), session("b", "backlog", "B"), session("c", "backlog", "C")],
    emptyOrders,
  )

  test("selects the first session when there is no current id", () => {
    const result = nextSessionID(columns, "backlog", undefined, 1)
    expect(result).toEqual({ column: "backlog", id: "a" })
  })

  test("moves to the next session", () => {
    const result = nextSessionID(columns, "backlog", "a", 1)
    expect(result).toEqual({ column: "backlog", id: "b" })
  })

  test("stops at the last session", () => {
    const result = nextSessionID(columns, "backlog", "c", 1)
    expect(result).toBeUndefined()
  })

  test("moves to the previous session", () => {
    const result = nextSessionID(columns, "backlog", "b", -1)
    expect(result).toEqual({ column: "backlog", id: "a" })
  })

  test("resets to the first session when current id is stale", () => {
    const result = nextSessionID(columns, "backlog", "missing", 1)
    expect(result).toEqual({ column: "backlog", id: "a" })
  })

  test("navigates through parent and child rows", () => {
    const nested = groupCardsByColumn(
      [
        session("parent", "backlog", "Parent", 2),
        session("child", "backlog", "Child", 3, "parent"),
        session("next", "backlog", "Next", 1),
      ],
      emptyOrders,
    )
    expect(nextSessionID(nested, "backlog", "parent", 1)).toEqual({ column: "backlog", id: "child" })
    expect(nextSessionID(nested, "backlog", "child", 1)).toEqual({ column: "backlog", id: "next" })
    expect(nextSessionID(nested, "backlog", "child", -1)).toEqual({ column: "backlog", id: "parent" })
  })
})

describe("reconcileOrders", () => {
  test("moves session ids into the column matching their status", () => {
    const sessions = [session("s1", "review", "Review"), session("s2", "backlog", "Backlog")]
    const orders = {
      backlog: ["s1", "s2"],
      in_progress: [],
      review: [],
      done: [],
    }
    expect(reconcileOrders(sessions, orders)).toEqual({
      backlog: ["s2"],
      in_progress: [],
      review: ["s1"],
      done: [],
    })
  })

  test("appends sessions missing from order lists", () => {
    const sessions = [session("s1", "done", "Done")]
    const orders = { backlog: [], in_progress: [], review: [], done: [] }
    expect(reconcileOrders(sessions, orders).done).toEqual(["s1"])
  })
})

function withFlag(base: BoardSession, flag: Record<string, unknown>): BoardSession {
  return {
    ...base,
    metadata: { kagan: { ...(base.metadata as { kagan: Record<string, unknown> }).kagan, ...flag } },
  }
}

// Map (helper failures) vs Set (awaiting input) seen-stores make a single generic signature awkward;
// `any` here is local test-infra plumbing, not a production contract.
function sharedDetectionCases(
  detect: (sessions: readonly BoardSession[], seen: any) => unknown[],
  makeSeen: () => any,
  status: ColumnType,
  flag: Record<string, unknown>,
  reNotifyFlag: Record<string, unknown>,
  expectedRow: Record<string, unknown>,
) {
  test("reports a new occurrence not previously seen", () => {
    const seen = makeSeen()
    const flagged = withFlag(session("s1", status, "Task"), flag)
    expect(detect([flagged], seen)).toEqual([expectedRow])
  })

  test("does not re-report the same occurrence on a subsequent poll", () => {
    const seen = makeSeen()
    const flagged = withFlag(session("s1", status, "Task"), flag)
    detect([flagged], seen)
    expect(detect([flagged], seen)).toEqual([])
  })

  test("forgets once cleared, so a later occurrence re-notifies", () => {
    const seen = makeSeen()
    detect([withFlag(session("s1", status, "Task"), flag)], seen)
    detect([session("s1", status, "Task")], seen)
    expect(seen.size).toBe(0)
    expect(detect([withFlag(session("s1", status, "Task"), reNotifyFlag)], seen)).toHaveLength(1)
  })

  test("ignores non-board sessions and subtasks even if flagged", () => {
    const seen = makeSeen()
    const subtask = withFlag(session("child", status, "Child", 0, "parent"), flag)
    const nonBoard = { ...withFlag(session("s1", status, "Task"), flag), metadata: { kagan: { status, ...flag } } }
    expect(detect([subtask, nonBoard], seen)).toEqual([])
  })
}

describe("detectNewHelperFailures", () => {
  sharedDetectionCases(
    detectNewHelperFailures,
    () => new Map<string, string>(),
    "review",
    { helperError: { role: "validator", message: "boom" } },
    { helperError: { role: "validator", message: "boom" } },
    { sessionID: "s1", taskNumber: undefined, role: "validator", message: "boom" },
  )

  test("reports a helperError not previously seen for that session, keyed as role:message", () => {
    const seen = new Map<string, string>()
    const failed = withFlag(session("s1", "review", "Task"), { helperError: { role: "validator", message: "boom" } })
    detectNewHelperFailures([failed], seen)
    expect(seen.get("s1")).toBe("validator:boom")
  })

  test("re-reports when the message changes (a new failure after a retry)", () => {
    const seen = new Map<string, string>()
    const withError = (message: string) =>
      withFlag(session("s1", "review", "Task"), { helperError: { role: "validator", message } })
    detectNewHelperFailures([withError("boom")], seen)
    const secondFailure = detectNewHelperFailures([withError("different error")], seen)
    expect(secondFailure).toEqual([
      { sessionID: "s1", taskNumber: undefined, role: "validator", message: "different error" },
    ])
  })
})

describe("detectNewAwaitingInput", () => {
  sharedDetectionCases(
    detectNewAwaitingInput,
    () => new Set<string>(),
    "in_progress",
    { awaitingInput: { id: "p1", title: "Run rm -rf?" } },
    { awaitingInput: { id: "p2", title: "y" } },
    { sessionID: "s1", taskNumber: undefined, permissionID: "p1", title: "Run rm -rf?" },
  )

  test("reports a wait not previously seen for that permission id, keyed by permission id", () => {
    const seen = new Set<string>()
    const waiting = withFlag(session("s1", "in_progress", "Task"), {
      awaitingInput: { id: "p1", title: "Run rm -rf?" },
    })
    detectNewAwaitingInput([waiting], seen)
    expect(seen.has("p1")).toBe(true)
  })
})

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

describe("noticeDuration", () => {
  test("defaults non-error variants to 5000ms", () => {
    expect(noticeDuration({ variant: "warning", title: "Kagan", message: "x" })).toBe(5000)
    expect(noticeDuration({ variant: "success", title: "Kagan", message: "x" })).toBe(5000)
    expect(noticeDuration({ title: "Kagan", message: "x" })).toBe(5000)
  })

  test("defaults the error variant to 10000ms", () => {
    expect(noticeDuration({ variant: "error", title: "Kagan", message: "x" })).toBe(10000)
  })

  test("honors an explicit duration override regardless of variant", () => {
    expect(noticeDuration({ variant: "error", title: "Kagan", message: "x", duration: 250 })).toBe(250)
  })
})

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
    const api = mockStoreApi({ sessions: [] })
    const store = createRoot(() => createBoardStore(api))
    store.notify({ variant: "error", title: "Kagan", message: "boom" })
    store.notify({ variant: "warning", title: "Kagan", message: "careful" })
    const [error, warning] = store.notices()
    expect(noticeDuration(error!)).toBeGreaterThan(noticeDuration(warning!))
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
        kagan: { status: "in_progress", boardTask: true, awaitingInput: { id: "p1", title: "Run rm -rf?" } },
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
      message: "s1 waiting on you — Run rm -rf?",
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
