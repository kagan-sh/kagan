/** @jsxImportSource @opentui/solid */
import { afterEach, beforeEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import { MockTreeSitterClient, type TestRendererSetup } from "@opentui/core/testing"
import type { JSX } from "solid-js"
import {
  answerMarkdown,
  configureIntakeMarkdownTreeSitter,
  decisionMarkdown,
  modeMarkdown,
  openIntakeDecisionDialog,
  taskRef,
} from "../../../src/tui/dialogs/intake-gate"
import { dialogContentWidth } from "../../../src/tui/dialogs/intake-gate-views"
import { attachRendererMockInput, mockSession, mockTuiApi } from "../../fixtures/api"

let renderSetup: TestRendererSetup | undefined
let treeSitter: MockTreeSitterClient | undefined

beforeEach(() => {
  treeSitter = new MockTreeSitterClient({ autoResolveTimeout: 0 })
  treeSitter.setMockResult({ highlights: [] })
  configureIntakeMarkdownTreeSitter(treeSitter)
})

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
  configureIntakeMarkdownTreeSitter(undefined)
  await treeSitter?.destroy()
  treeSitter = undefined
})

describe("intake gate markdown helpers", () => {
  test("composes decision and answer markdown sections", () => {
    const decision = { id: "d1", question: "Which name?", assumption: "Use `Kagan`", required: true }
    expect(decisionMarkdown(decision)).toContain("## Assumption")
    expect(decisionMarkdown(decision)).toContain("Use `Kagan`")
    expect(decisionMarkdown(decision)).toContain("## Question")
    expect(answerMarkdown(decision)).toContain("### Overriding assumption")
    expect(modeMarkdown("Needs a human.", "assisted")).toBe("## Why assisted\n\nNeeds a human.")
  })

  test("dialogContentWidth follows host large panel, not terminal width", () => {
    expect(dialogContentWidth(120)).toBe(84)
    expect(dialogContentWidth(70)).toBe(64)
    expect(dialogContentWidth(40)).toBe(34)
  })

  test("taskRef prefers task number", () => {
    expect(
      taskRef(mockSession("s1", "backlog", "Title", 0, undefined, { metadata: { kagan: { taskNumber: 4 } } })),
    ).toBe("#4")
  })
})

describe("openIntakeDecisionDialog", () => {
  test("renders assumption and question with approve actions", async () => {
    let rendered: (() => JSX.Element) | undefined
    const api = mockTuiApi({
      renderer: { width: 60, height: 28 },
      ui: {
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => {
            rendered = render as () => JSX.Element
          },
        },
      },
    })
    openIntakeDecisionDialog(api, {
      session: mockSession("s1", "backlog", "refine docs", 0, undefined, {
        metadata: { kagan: { taskNumber: 1, boardTask: true } },
      }),
      index: 0,
      total: 1,
      decision: {
        id: "d1",
        question: "Which name should the README use?",
        assumption: "Do not implement until a canonical name is chosen.",
        required: true,
      },
      onApprove: () => {},
      onReject: () => {},
      onCancel: () => {},
    })
    renderSetup = await testRender(rendered!, { width: 60, height: 28 })
    await renderSetup.flush()
    await Bun.sleep(40)
    treeSitter?.resolveAllHighlightOnce()
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Intake decision (1/1)")
    expect(frame).toContain("#1")
    expect(frame).toContain("Assumption")
    expect(frame).toContain("canonical name")
    expect(frame).toContain("Which name should the README use?")
    expect(frame).toContain("Accept assumption")
    expect(frame).toContain("Override with answer")
  })

  test("wraps long question inside the host large dialog width", async () => {
    let rendered: (() => JSX.Element) | undefined
    const question =
      'Should "refine docs" be limited to replacing the root README with end-user documentation for the current calculator, or should it also introduce a docs directory?'
    const api = mockTuiApi({
      renderer: { width: 120, height: 32 },
      ui: {
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => {
            rendered = render as () => JSX.Element
          },
        },
      },
    })
    openIntakeDecisionDialog(api, {
      session: mockSession("s1", "backlog", "refine docs", 0, undefined, {
        metadata: { kagan: { taskNumber: 1, boardTask: true } },
      }),
      index: 0,
      total: 1,
      decision: {
        id: "d1",
        question,
        assumption: "Limit this focused task to the root README.",
        required: true,
      },
      onApprove: () => {},
      onReject: () => {},
      onCancel: () => {},
    })
    renderSetup = await testRender(rendered!, { width: 120, height: 32 })
    await renderSetup.flush()
    await Bun.sleep(40)
    treeSitter?.resolveAllHighlightOnce()
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("docs directory")
    expect(frame).toContain("Accept assumption")
    // Without dialog-sized wrap, this question stays on one ~160-char line and clips at the panel edge.
    expect(frame.includes('Should "refine docs"') && frame.includes("docs directory")).toBe(true)
    const startLine = frame.split("\n").find((line) => line.includes('Should "refine docs"'))
    expect(startLine?.includes("docs directory")).toBe(false)
  })

  test("enter approves the selected action", async () => {
    let approved = false
    let rendered: (() => JSX.Element) | undefined
    const api = mockTuiApi({
      renderer: { width: 100, height: 24 },
      ui: {
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => {
            rendered = render as () => JSX.Element
          },
        },
      },
    })
    openIntakeDecisionDialog(api, {
      session: mockSession("s1", "backlog", "Task"),
      index: 0,
      total: 1,
      decision: { id: "d1", question: "Q?", assumption: "A", required: true },
      onApprove: () => {
        approved = true
      },
      onReject: () => {},
      onCancel: () => {},
    })
    renderSetup = await testRender(rendered!, { width: 100, height: 24, kittyKeyboard: true })
    attachRendererMockInput(api, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.waitFor(() => approved)
    expect(approved).toBe(true)
  })
})
