/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { TestRendererSetup } from "@opentui/core/testing"
import { Column } from "../../../src/tui/board/column"
import type { BoardStore } from "../../../src/tui/board/store"
import type { BoardCard, BoardSession } from "../../../src/tui/types"
import { mockSession, mockTuiApi } from "../../fixtures/api"

function store(overrides: Partial<BoardStore> = {}): BoardStore {
  return { inProgressCap: 2, sendBackStopThreshold: 3, checkCommand: undefined, ...overrides } as BoardStore
}

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

function session(id: string, title: string, parentID?: string): BoardSession {
  return mockSession(id, "in_progress", title, 0, parentID, { metadata: undefined })
}

function card(sessionItem: BoardSession, children: BoardSession[] = []): BoardCard {
  return { session: sessionItem, children }
}

describe("Column", () => {
  test("shows count and cap in the header", async () => {
    renderSetup = await testRender(
      () => (
        <Column
          api={mockTuiApi()}
          store={store()}
          column="in_progress"
          cards={[card(session("a", "A")), card(session("b", "B"))]}
          selectedID="a"
          onSelect={() => {}}
        />
      ),
      { width: 30, height: 10 },
    )
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toMatch(/In Progress\s+△ 2\/2/)
  })

  test("highlights the header when at the cap", async () => {
    renderSetup = await testRender(
      () => (
        <Column
          api={mockTuiApi()}
          store={store()}
          column="in_progress"
          cards={[card(session("a", "A")), card(session("b", "B"))]}
          selectedID="a"
          onSelect={() => {}}
        />
      ),
      { width: 30, height: 10 },
    )
    await renderSetup.flush()
    const { lines } = renderSetup.captureSpans()
    const headerSpan = lines.flatMap((line) => line.spans).find((span) => span.text.includes("Progress"))
    expect(headerSpan).toBeDefined()
    expect(headerSpan?.fg.r).toBe(1)
    expect(headerSpan?.fg.g).toBe(1)
    expect(headerSpan?.fg.b).toBe(0)
    expect(headerSpan?.fg.a).toBe(1)
  })

  test("renders nested subtasks under the parent card", async () => {
    renderSetup = await testRender(
      () => (
        <Column
          api={mockTuiApi()}
          store={store()}
          column="backlog"
          cards={[card(session("parent", "Parent task"), [session("child", "Review i18n a11y", "parent")])]}
          selectedID="parent"
          onSelect={() => {}}
        />
      ),
      { width: 40, height: 16 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Parent task")
    expect(frame).toContain("Review i18n a11y")
  })

  test("highlights a selected subtask", async () => {
    renderSetup = await testRender(
      () => (
        <Column
          api={mockTuiApi()}
          store={store()}
          column="backlog"
          cards={[
            card(session("parent", "Parent task"), [
              session("child", "Review i18n a11y (@general subagent)", "parent"),
            ]),
          ]}
          selectedID="child"
          onSelect={() => {}}
        />
      ),
      { width: 40, height: 16 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Review i18n a11y")
    expect(frame).not.toContain("(@general subagent)")
    const { lines } = renderSetup.captureSpans()
    const subtaskSpan = lines.flatMap((line) => line.spans).find((span) => span.text.includes("Review i18n a11y"))
    expect(subtaskSpan).toBeDefined()
    expect(subtaskSpan?.fg.r).toBe(0)
    expect(subtaskSpan?.fg.g).toBe(1)
    expect(subtaskSpan?.fg.b).toBe(1)
  })

  test("shows a collapsed summary when the parent is not focused", async () => {
    renderSetup = await testRender(
      () => (
        <Column
          api={mockTuiApi()}
          store={store()}
          column="backlog"
          cards={[
            card(session("parent", "Parent task"), [session("child", "Review i18n a11y", "parent")]),
            card(session("other", "Other task")),
          ]}
          selectedID="other"
          onSelect={() => {}}
        />
      ),
      { width: 40, height: 16 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("1 subtask")
    expect(frame).toContain("Review i18n a11y")
  })
})
