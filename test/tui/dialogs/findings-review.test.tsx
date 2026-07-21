/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { JSX } from "solid-js"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { openFindingsReviewDialog } from "../../../src/tui/dialogs/findings-review/panel"
import type { Finding } from "../../../src/domain/task/findings"
import type { BoardSession } from "../../../src/tui/types"
import { attachRendererMockInput, mockSession, mockTuiApi } from "../../fixtures/api"

let renderSetup: TestRendererSetup | undefined

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
})

function session(findings: Finding[] | undefined, extra: Record<string, unknown> = {}): BoardSession {
  return mockSession("s1", "review", "Task", 0, undefined, {
    metadata: { kagan: { boardTask: true, validatorOutcome: "ran", findings, ...extra } },
  })
}

function api(overrides: { update?: (args: unknown) => void; metadata?: Record<string, unknown> } = {}): TuiPluginApi {
  return mockTuiApi({
    ui: {
      toast: () => {},
      dialog: { open: true, setSize: () => {}, clear: () => {}, replace: () => {} },
    },
    client: {
      session: {
        get: async () => ({ data: { metadata: overrides.metadata ?? {} } }),
        update: async (args: unknown) => {
          overrides.update?.(args)
        },
      },
    },
  } as unknown as Partial<TuiPluginApi>)
}

function boardStore(sessions: BoardSession[] = []) {
  return { refresh: async () => {}, sessions: () => sessions }
}

function mountDialog(
  boardApi: TuiPluginApi,
  task: BoardSession,
  handlers: { onApprove?: (session: BoardSession) => void; onSendBack?: () => void } = {},
  checkCommand?: string,
) {
  let rendered: (() => JSX.Element) | undefined
  boardApi.ui.dialog.replace = (render: () => unknown) => {
    rendered = render as () => JSX.Element
  }
  openFindingsReviewDialog(boardApi, boardStore() as never, task, checkCommand, {
    onApprove: handlers.onApprove ?? (() => {}),
    onSendBack: handlers.onSendBack ?? (() => {}),
  })
  if (!rendered) throw new Error("expected dialog.replace to be called")
  return rendered
}

describe("openFindingsReviewDialog", () => {
  test("renders every finding sorted by confidence with ruling state", async () => {
    const findings: Finding[] = [
      { id: "f1", summary: "Leaked handle", category: "bug", severity: "high", confidence: 10 },
      {
        id: "f2",
        summary: "Style nit",
        category: "uncertainty",
        severity: "low",
        confidence: 2,
        resolution: "ignored",
        note: "Confirmed intentional style choice with the author",
      },
    ]
    const boardApi = api()
    renderSetup = await testRender(mountDialog(boardApi, session(findings)), { width: 160, height: 24 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Leaked handle")
    expect(frame).toContain("! untriaged")
    expect(frame).toContain("⊘ ignored (Confirmed intentional style choice with the author)")
  })

  test("shows approve blocked while findings are pending", async () => {
    const boardApi = api()
    renderSetup = await testRender(mountDialog(boardApi, session([{ id: "f1", summary: "Leaked handle" }])), {
      width: 100,
      height: 24,
    })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Approve — triage findings first")
    expect(frame).toContain("1 finding(s) need triage — enter opens a finding")
  })

  test("shows a clean state with approve enabled when there are no findings", async () => {
    const boardApi = api()
    renderSetup = await testRender(mountDialog(boardApi, session([])), { width: 100, height: 24 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("No findings — review is clean.")
    expect(frame).toContain("Approve — ready to merge")
    expect(frame).toContain("▸ Approve & merge — a")
  })

  test("escape closes the dialog", async () => {
    let cleared = false
    const boardApi = api()
    ;(boardApi.ui.dialog as unknown as { clear: () => void }).clear = () => {
      cleared = true
    }
    renderSetup = await testRender(mountDialog(boardApi, session([{ id: "f1", summary: "x" }])), {
      width: 100,
      height: 24,
      kittyKeyboard: true,
    })
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEscape()
    await renderSetup.waitFor(() => cleared)
    expect(cleared).toBe(true)
  })

  test("s sends the task back", async () => {
    let sentBack = false
    const boardApi = api()
    renderSetup = await testRender(
      mountDialog(boardApi, session([{ id: "f1", summary: "x" }]), { onSendBack: () => (sentBack = true) }),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("s")
    await renderSetup.waitFor(() => sentBack)
    expect(sentBack).toBe(true)
  })

  test("a approves a clean session", async () => {
    let approved: BoardSession | undefined
    const boardApi = api()
    renderSetup = await testRender(
      mountDialog(boardApi, session([]), {
        onApprove: (value) => {
          approved = value
        },
      }),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("a")
    await renderSetup.waitFor(() => approved !== undefined)
    expect(approved?.id).toBe("s1")
  })

  test("passes checkCommand so the no-check overlay is hidden when a command is set", async () => {
    const task = session([], {
      intake: {
        understanding: "x",
        decisions: [],
        mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
      },
    })
    const boardApi = api()
    renderSetup = await testRender(mountDialog(boardApi, task, {}, "bun run check"), { width: 80, height: 12 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame.replace(/\s+/g, " ")).toContain("No trusted check and the blast radius is high.")
    expect(frame.replace(/\s+/g, " ")).not.toContain("no automatic check configured")
  })
})
