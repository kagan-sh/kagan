/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { TestRendererSetup } from "@opentui/core/testing"
import { CardShell } from "../../../src/tui/board/card/body"
import type { BoardSession } from "../../../src/tui/types"
import { mockSession as buildSession, mockTuiApi } from "../../fixtures/api"

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

function session(
  props: Partial<BoardSession> & {
    id: string
    title: string
    updated?: number
    kaganStatus?: BoardSession["kaganStatus"]
    parentID?: string
  },
): BoardSession {
  return buildSession(props.id, props.kaganStatus ?? "backlog", props.title, props.updated ?? 0, props.parentID, {
    summary: props.summary,
    metadata: props.metadata,
  })
}

function Card(props: Omit<Parameters<typeof CardShell>[0], "renderedAt">) {
  return <CardShell {...props} renderedAt={Date.now()} />
}

describe("Card", () => {
  test("renders the session title", async () => {
    renderSetup = await testRender(
      () => <Card api={mockTuiApi()} session={session({ id: "s1", title: "Hello" })} onSelect={() => {}} />,
      { width: 40, height: 5 },
    )
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("Hello")
  })

  test("prefixes the title with the task number", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({ id: "s1", title: "Hello", metadata: { kagan: { taskNumber: 4 } } })}
          onSelect={() => {}}
        />
      ),
      { width: 40, height: 5 },
    )
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("#4 Hello")
  })

  test("draws the left status bar", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({ id: "s1", title: "T", kaganStatus: "in_progress" })}
          onSelect={() => {}}
        />
      ),
      { width: 20, height: 5 },
    )
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("┃")
  })

  test("marks an intake-ready backlog task as ready", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({
            id: "s1",
            title: "Ready task",
            metadata: { kagan: { status: "backlog", boardTask: true, intakeOutcome: "ran" } },
          })}
          onSelect={() => {}}
        />
      ),
      { width: 50, height: 5 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("intake ok")
  })

  test("shows findings badges once the validator has run", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({
            id: "s1",
            title: "Reviewed",
            kaganStatus: "review",
            metadata: {
              kagan: { status: "review", validatorOutcome: "ran", findings: [{ id: "f1", summary: "leak" }] },
            },
          })}
          onSelect={() => {}}
        />
      ),
      { width: 50, height: 5 },
    )
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("1 to triage")
  })

  test("shows a collapsed subtask summary when the family is not focused", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({ id: "parent", title: "Parent task" })}
          children={[
            session({ id: "c1", title: "Review infra security (@general subagent)", parentID: "parent" }),
            session({ id: "c2", title: "Review code quality (@general subagent)", parentID: "parent" }),
          ]}
          selectedID="other"
          onSelect={() => {}}
        />
      ),
      { width: 50, height: 8 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("2 subtasks")
    expect(frame).not.toContain("(@general subagent)")
  })

  test("expands subtasks when a child is selected", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({ id: "parent", title: "Parent task" })}
          children={[session({ id: "child", title: "Review i18n a11y (@general subagent)", parentID: "parent" })]}
          selectedID="child"
          onSelect={() => {}}
        />
      ),
      { width: 40, height: 10 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Review i18n a11y")
    expect(frame).not.toContain("(@general subagent)")
  })

  test("highlights a selected subtask with a marker", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({ id: "parent", title: "Parent task" })}
          children={[session({ id: "child", title: "Selected subtask", parentID: "parent" })]}
          selectedID="child"
          onSelect={() => {}}
        />
      ),
      { width: 40, height: 10 },
    )
    await renderSetup.flush()
    const { lines } = renderSetup.captureSpans()
    const titleSpan = lines.flatMap((line) => line.spans).find((span) => span.text.includes("Selected subtask"))
    expect(titleSpan).toBeDefined()
    expect(titleSpan?.fg.r).toBe(0)
    expect(titleSpan?.fg.g).toBe(1)
    expect(titleSpan?.fg.b).toBe(1)
  })

  test("shows the mode rationale text when the card is selected", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({
            id: "s1",
            title: "Task",
            metadata: {
              kagan: {
                status: "backlog",
                boardTask: true,
                intakeOutcome: "ran",
                intake: {
                  understanding: "x",
                  decisions: [],
                  mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
                },
              },
            },
          })}
          selectedID="s1"
          checkCommand={undefined}
          onSelect={() => {}}
        />
      ),
      { width: 80, height: 10 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame().replace(/[\s┃]+/g, " ")
    expect(frame).toContain("No trusted check and the blast radius is high.")
    expect(frame).toContain("(no automatic check configured - lean assisted)")
  })

  test("hides the mode rationale text when the card is not selected", async () => {
    renderSetup = await testRender(
      () => (
        <Card
          api={mockTuiApi()}
          session={session({
            id: "s1",
            title: "Task",
            metadata: {
              kagan: {
                status: "backlog",
                boardTask: true,
                intakeOutcome: "ran",
                intake: {
                  understanding: "x",
                  decisions: [],
                  mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
                },
              },
            },
          })}
          selectedID="other"
          checkCommand={undefined}
          onSelect={() => {}}
        />
      ),
      { width: 80, height: 10 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame().replace(/[\s┃]+/g, " ")
    expect(frame).not.toContain("No trusted check and the blast radius")
  })
})
