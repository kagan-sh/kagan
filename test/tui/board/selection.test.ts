import { describe, expect, test } from "bun:test"
import { createSignal } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { ColumnType } from "../../../src/domain/task/types"
import {
  ensureSelection,
  selectColumnStep,
  selectRootStep,
  selectStep,
  type StoreState,
} from "../../../src/tui/board/store/selection"
import type { BoardCard, BoardSession } from "../../../src/tui/types"
import { mockSession } from "../../fixtures/api"

function card(id: string, status: ColumnType, children: BoardSession[] = []): BoardCard {
  return { session: mockSession(id, status, id), children }
}

function emptyColumns(): Record<ColumnType, BoardCard[]> {
  return { backlog: [], in_progress: [], review: [], done: [] }
}

function mockState(init: {
  columns: Record<ColumnType, BoardCard[]>
  selectedID?: string
  selectedColumn?: ColumnType
}): StoreState {
  const [selectedID, setSelectedID] = createSignal<string | undefined>(init.selectedID)
  const [selectedColumn, setSelectedColumn] = createSignal<ColumnType>(init.selectedColumn ?? "backlog")
  const sessions = Object.values(init.columns).flatMap((cards) =>
    cards.flatMap((item) => [item.session, ...item.children]),
  )
  return {
    api: {} as TuiPluginApi,
    sessions: () => sessions,
    setSessions: () => {},
    selectedID,
    setSelectedID,
    selectedColumn,
    setSelectedColumn,
    filter: () => "",
    setFilterSignal: () => {},
    orders: () => ({ backlog: [], in_progress: [], review: [], done: [] }),
    setOrders: () => {},
    columns: () => init.columns,
    notify: () => {},
    toastError: () => {},
    runWithToast: async (fn) => fn(),
    refreshState: { started: 0, completed: 0, helperFailuresSeen: new Map(), awaitingPermissionsSeen: new Set() },
  }
}

describe("board selection", () => {
  test("selectStep follows the highlighted card when selectedColumn is stale", () => {
    const columns = emptyColumns()
    columns.review = [card("a", "review"), card("b", "review")]
    const state = mockState({ columns, selectedID: "a", selectedColumn: "backlog" })

    selectStep(state, 1)

    expect(state.selectedID()).toBe("b")
    expect(state.selectedColumn()).toBe("review")
  })

  test("selectColumnStep into an empty column clears the previous card selection", () => {
    const columns = emptyColumns()
    columns.review = [card("a", "review")]
    const state = mockState({ columns, selectedID: "a", selectedColumn: "review" })

    selectColumnStep(state, 1)

    expect(state.selectedColumn()).toBe("done")
    expect(state.selectedID()).toBeUndefined()
  })

  test("ensureSelection picks the first card outside an empty default column", () => {
    const columns = emptyColumns()
    columns.in_progress = [card("wip", "in_progress")]
    const state = mockState({ columns })

    ensureSelection(state)

    expect(state.selectedID()).toBe("wip")
    expect(state.selectedColumn()).toBe("in_progress")
  })

  test("ensureSelection resyncs selectedColumn to the column holding the selection", () => {
    const columns = emptyColumns()
    columns.review = [card("a", "review")]
    const state = mockState({ columns, selectedID: "a", selectedColumn: "done" })

    ensureSelection(state)

    expect(state.selectedID()).toBe("a")
    expect(state.selectedColumn()).toBe("review")
  })

  test("selectRootStep cycles root cards across columns and skips children", () => {
    const child = mockSession("child", "backlog", "Child", 0, "a")
    const columns = emptyColumns()
    columns.backlog = [card("a", "backlog", [child]), card("b", "backlog"), card("c", "backlog")]
    columns.in_progress = [card("d", "in_progress"), card("e", "in_progress"), card("f", "in_progress")]
    const state = mockState({ columns, selectedID: "a", selectedColumn: "backlog" })

    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("b")
    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("c")
    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("d")
    expect(state.selectedColumn()).toBe("in_progress")
    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("e")
    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("f")
    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("a")
    expect(state.selectedColumn()).toBe("backlog")
  })

  test("selectRootStep from a child lands on the next root after its parent", () => {
    const child = mockSession("child", "backlog", "Child", 0, "c")
    const columns = emptyColumns()
    columns.backlog = [card("a", "backlog"), card("b", "backlog"), card("c", "backlog", [child])]
    columns.in_progress = [card("d", "in_progress")]
    const state = mockState({ columns, selectedID: "child", selectedColumn: "backlog" })

    selectRootStep(state, 1)
    expect(state.selectedID()).toBe("d")
    expect(state.selectedColumn()).toBe("in_progress")

    selectRootStep(state, -1)
    expect(state.selectedID()).toBe("c")
    expect(state.selectedColumn()).toBe("backlog")
  })
})
