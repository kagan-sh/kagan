/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { JSX } from "solid-js"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { openIntakeNotesView, openTaskDetailsView } from "../../../src/tui/dialogs/task-details"
import type { Intake } from "../../../src/domain/task/intake"
import type { TaskDetails } from "../../../src/tui/dialogs/task-details-sections"
import { mockTuiApi } from "../../fixtures/api"

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

function api(): { boardApi: TuiPluginApi; sizes: string[]; mount: () => () => JSX.Element } {
  const sizes: string[] = []
  let rendered: (() => JSX.Element) | undefined
  const boardApi = mockTuiApi({
    ui: {
      toast: () => {},
      dialog: {
        open: true,
        setSize: (size: string) => {
          sizes.push(size)
        },
        clear: () => {},
        replace: (render: () => unknown) => {
          rendered = render as () => JSX.Element
        },
      },
    },
    renderer: { width: 100, height: 24 },
  } as unknown as Partial<TuiPluginApi>)
  return {
    boardApi,
    sizes,
    mount: () => {
      if (!rendered) throw new Error("expected dialog.replace to be called")
      return rendered
    },
  }
}

const longDecision =
  "Should the docs stop at the end-user calculator, or should it also introduce developer/API surfaces and packaging notes that stretch past the dialog edge?"

function fatDetails(): TaskDetails {
  return {
    title: "refine docs",
    status: "review",
    taskNumber: 1,
    report: "Updated README.md with user-facing calculator documentation.",
    description: "Ship clearer docs.",
    baseBranch: "chore/kagan-config",
    generation: 1,
    approved: false,
    findings: [],
    priorTriage: [],
    intake: {
      understanding: "Document the calculator for end users.",
      decisions: [
        {
          id: "d1",
          question: longDecision,
          assumption: "End-user only for this pass.",
          required: true,
          resolution: "approved",
        },
      ],
    },
    diffStats: [
      { file: "README.md", additions: 40, deletions: 2, status: "modified" },
      { file: "docs/concepts/task-lifecycle.md", additions: 12, deletions: 1, status: "modified" },
    ],
  }
}

describe("openTaskDetailsView", () => {
  test("opens large, pins the summary, and keeps the scroll footer above long intake", async () => {
    const { boardApi, sizes, mount } = api()
    openTaskDetailsView(boardApi, fatDetails(), "#1 refine docs")
    renderSetup = await testRender(mount(), { width: 100, height: 24 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(sizes).toEqual(["large"])
    expect(frame).toContain("#1 refine docs")
    expect(frame).toContain("review · Generation 1 · not approved · base: chore/kagan-config")
    expect(frame).toContain("↑↓ scroll")
    expect(frame).toContain("esc close")
    expect(frame).toContain("Report")
    expect(frame).toContain("Changed files (2)")
  })
})

describe("openIntakeNotesView", () => {
  test("opens large with a scrollable intake body", async () => {
    const intake: Intake = {
      understanding: "Document the calculator for end users.",
      decisions: [
        {
          id: "d1",
          question: longDecision,
          assumption: "End-user only.",
          required: true,
          resolution: "approved",
        },
      ],
    }
    const { boardApi, sizes, mount } = api()
    openIntakeNotesView(boardApi, intake)
    renderSetup = await testRender(mount(), { width: 100, height: 24 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(sizes).toEqual(["large"])
    expect(frame).toContain("Intake notes")
    expect(frame).toContain("↑↓ scroll")
    expect(frame).toContain("Document the calculator for end users.")
  })
})
