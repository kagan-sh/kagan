/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import { testRender } from "@opentui/solid"
import type { JSX } from "solid-js"
import type { TestRendererSetup } from "@opentui/core/testing"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { detailHeader, FindingsReview, locationMarker, openFindingsReviewDialog } from "../src/findings-review"
import type { Finding } from "../src/task"
import type { BoardSession } from "../src/types"
import { attachRendererMockInput, mockSession, mockTuiApi } from "./fixtures/api"

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

function store(sessions: BoardSession[] = []) {
  return { refresh: async () => {}, sessions: () => sessions } as unknown as Parameters<
    typeof FindingsReview
  >[0]["store"]
}

describe("detailHeader", () => {
  test("formats position, category, severity, and confidence", () => {
    expect(detailHeader({ id: "f1", summary: "x", category: "bug", severity: "high", confidence: 7 }, 0, 3)).toBe(
      "finding 1/3 · bug · high · confidence 7/10",
    )
  })

  test("falls back to defaults for an unscored, uncategorized finding", () => {
    expect(detailHeader({ id: "f1", summary: "x" }, 1, 2)).toBe("finding 2/2 · finding · unscored · confidence ?/10")
  })
})

describe("locationMarker", () => {
  test("renders a marker for a finding whose citation didn't verify", () => {
    expect(locationMarker({ id: "f1", summary: "x", outOfDiff: true })).toBe("⚠ not found in diff")
  })

  test("is undefined for a verified finding", () => {
    expect(locationMarker({ id: "f1", summary: "x" })).toBeUndefined()
  })

  test("is undefined when there is no current finding", () => {
    expect(locationMarker(undefined)).toBeUndefined()
  })
})

describe("FindingsReview — list mode", () => {
  test("renders every finding sorted by confidence, with severity word, confidence bar, category, and ruling state", async () => {
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
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={api()}
          store={store()}
          session={session(findings)}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 160, height: 24 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("high")
    expect(frame).toContain("low")
    expect(frame).toContain("██████████")
    expect(frame).toContain("██░░░░░░░░")
    expect(frame).toContain("bug")
    expect(frame).not.toContain("[bug]")
    expect(frame).toContain("Leaked handle")
    expect(frame).toContain("uncertainty")
    expect(frame).toContain("! untriaged")
    expect(frame).toContain("⊘ ignored (Confirmed intentional style choice with the author)")
  })

  test("shows approve blocked with the deny reason while findings are pending", async () => {
    const findings: Finding[] = [{ id: "f1", summary: "Leaked handle" }]
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={api()}
          store={store()}
          session={session(findings)}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Approve — triage findings first")
    expect(frame).toContain("a approve")
    expect(frame).not.toContain("approve & merge")
    expect(frame).toContain("(1 finding(s) need triage)")
  })

  test("titles the dialog with the task number when the session has one", async () => {
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={api()}
          store={store()}
          session={session([], { taskNumber: 7 })}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("Approve #7 — triage findings first")
  })

  test("shows a clean state with approve enabled when there are no findings", async () => {
    renderSetup = await testRender(
      () => (
        <FindingsReview api={api()} store={store()} session={session([])} onApprove={() => {}} onSendBack={() => {}} />
      ),
      { width: 100, height: 24 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("No findings — review is clean.")
    expect(frame).toContain("a approve & merge")
    expect(frame).not.toContain("✗")
  })

  test("escape closes the dialog", async () => {
    let cleared = false
    const boardApi = api()
    ;(boardApi.ui.dialog as unknown as { clear: () => void }).clear = () => {
      cleared = true
    }
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([{ id: "f1", summary: "x" }])}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24, kittyKeyboard: true },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEscape()
    await renderSetup.waitFor(() => cleared)
    expect(cleared).toBe(true)
  })

  test("s sends the task back and closes the dialog", async () => {
    let sentBack = false
    const boardApi = api()
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([{ id: "f1", summary: "x" }])}
          onApprove={() => {}}
          onSendBack={() => {
            sentBack = true
          }}
        />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("s")
    await renderSetup.waitFor(() => sentBack)
    expect(sentBack).toBe(true)
  })

  test("a does not approve while blocked", async () => {
    let approved: BoardSession | undefined
    const boardApi = api()
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([{ id: "f1", summary: "x" }])}
          onApprove={(s) => {
            approved = s
          }}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("a")
    await renderSetup.flush()
    expect(approved).toBeUndefined()
  })

  test("a approves the clean session and closes the dialog", async () => {
    let approved: BoardSession | undefined
    const boardApi = api()
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([])}
          onApprove={(s) => {
            approved = s
          }}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("a")
    await renderSetup.waitFor(() => approved !== undefined)
    expect(approved?.id).toBe("s1")
  })
})

// Detail-mode assertions verify behavior through side effects (the persisted update / refusal to
// persist) rather than through captureCharFrame(), because this harness does not always re-paint
// after every post-mount state change.
describe("FindingsReview — detail mode", () => {
  const finding: Finding = {
    id: "f1",
    summary: "Possible race condition",
    detail: "The refresh loop reads `seen` before the debounce settles, so two calls can both pass the guard.",
    location: "src/store.ts:142",
    category: "bug",
    severity: "high",
    confidence: 7,
  }

  test("refuses an ignore ruling with an empty note and does not persist", async () => {
    let updateCalls = 0
    const boardApi = api({ update: () => updateCalls++ })
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([finding])}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(updateCalls).toBe(0)
  })

  test("refuses a clarify ruling with an insubstantive note and does not persist", async () => {
    let updateCalls = 0
    const boardApi = api({ update: () => updateCalls++ })
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([finding])}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    await renderSetup.mockInput.typeText("ok")
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(updateCalls).toBe(0)
  })

  test("refuses an intended ruling on a high finding with an insubstantive note and does not persist", async () => {
    let updateCalls = 0
    const boardApi = api({ update: () => updateCalls++ })
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={boardApi}
          store={store()}
          session={session([finding])}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    await renderSetup.mockInput.typeText("n/a")
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(updateCalls).toBe(0)
  })

  test("persists a clarify ruling with a substantive note and returns to list", async () => {
    let updateArgs: unknown
    const target = session([finding])
    const boardApi = api({ update: (args) => (updateArgs = args), metadata: target.metadata })
    renderSetup = await testRender(
      () => (
        <FindingsReview api={boardApi} store={store()} session={target} onApprove={() => {}} onSendBack={() => {}} />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    await renderSetup.mockInput.typeText("This is expected because the caller holds the lock")
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(updateArgs).toBeDefined()
    const kagan = (updateArgs as { metadata: { kagan: Record<string, unknown> } }).metadata.kagan
    const persisted = (kagan.findings as Finding[])[0]
    expect(persisted?.resolution).toBe("clarified")
    expect(persisted?.note).toBe("This is expected because the caller holds the lock")
  })

  test("persists an ignore ruling with a substantive note and returns to list", async () => {
    let updateArgs: unknown
    const target = session([finding])
    const boardApi = api({ update: (args) => (updateArgs = args), metadata: target.metadata })
    renderSetup = await testRender(
      () => (
        <FindingsReview api={boardApi} store={store()} session={target} onApprove={() => {}} onSendBack={() => {}} />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    await renderSetup.mockInput.typeText("Confirmed safe: debounce already covers this")
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(updateArgs).toBeDefined()
    const kagan = (updateArgs as { metadata: { kagan: Record<string, unknown> } }).metadata.kagan
    const persisted = (kagan.findings as Finding[])[0]
    expect(persisted?.resolution).toBe("ignored")
    expect(persisted?.note).toBe("Confirmed safe: debounce already covers this")
  })

  test("re-opening an already-ruled finding pre-fills its note, so re-committing without retyping keeps it", async () => {
    const ruled: Finding = { ...finding, resolution: "clarified", note: "Timeout defaults to 30s per the config" }
    let updateArgs: unknown
    const target = session([ruled])
    const boardApi = api({ update: (args) => (updateArgs = args), metadata: target.metadata })
    renderSetup = await testRender(
      () => (
        <FindingsReview api={boardApi} store={store()} session={target} onApprove={() => {}} onSendBack={() => {}} />
      ),
      { width: 100, height: 24 },
    )
    attachRendererMockInput(boardApi, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    // Re-rule as "clarified" again without typing anything new: this only succeeds (the
    // substantive-note gate only passes) if openDetail() pre-filled note() from finding.note.
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressTab()
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(updateArgs).toBeDefined()
    const kagan = (updateArgs as { metadata: { kagan: Record<string, unknown> } }).metadata.kagan
    const persisted = (kagan.findings as Finding[])[0]
    expect(persisted?.resolution).toBe("clarified")
    expect(persisted?.note).toBe("Timeout defaults to 30s per the config")
  })
})

describe("FindingsReview mode rationale", () => {
  test("renders the mode rationale in the header with the no-check overlay", async () => {
    renderSetup = await testRender(
      () => (
        <FindingsReview
          api={api()}
          store={store()}
          session={session([], {
            intake: {
              understanding: "x",
              decisions: [],
              mode: { recommended: "manual", rationale: "The human must understand every line of the change." },
            },
          })}
          checkCommand={undefined}
          onApprove={() => {}}
          onSendBack={() => {}}
        />
      ),
      { width: 80, height: 12 },
    )
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame.replace(/\s+/g, " ")).toContain("The human must understand every line of the change.")
    expect(frame.replace(/\s+/g, " ")).toContain("(no automatic check configured - lean assisted)")
  })

  test("openFindingsReviewDialog passes checkCommand so the overlay is hidden when a command is set", async () => {
    const task = session([], {
      intake: {
        understanding: "x",
        decisions: [],
        mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
      },
    })
    const boardApi = api()
    let rendered: (() => unknown) | undefined
    boardApi.ui.dialog.replace = (render: () => unknown) => {
      rendered = render
    }

    openFindingsReviewDialog(boardApi, store(), task, "bun run check", {
      onApprove: () => {},
      onSendBack: () => {},
    })
    expect(rendered).toBeDefined()

    renderSetup = await testRender(rendered as () => JSX.Element, { width: 80, height: 12 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame.replace(/\s+/g, " ")).toContain("No trusted check and the blast radius is high.")
    expect(frame.replace(/\s+/g, " ")).not.toContain("no automatic check configured")
  })
})
