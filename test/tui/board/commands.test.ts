/** @jsxImportSource @opentui/solid */
import { afterEach, beforeEach, describe, expect, mock, test } from "bun:test"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { MockTreeSitterClient, type TestRendererSetup } from "@opentui/core/testing"
import { testRender } from "@opentui/solid"
import type { JSX } from "solid-js"
import type { BoardSession } from "../../../src/tui/types"
import { SETTINGS_ROUTE } from "../../../src/tui/types"
import type { BoardStore } from "../../../src/tui/board/commands"
import { configureIntakeMarkdownTreeSitter } from "../../../src/tui/dialogs/intake-gate"
import { attachRendererMockInput, mockSessionClient, mockTheme, mockTuiApi } from "../../fixtures/api"

import { BOARD_BINDINGS, createBoardCommands, footerHints } from "../../../src/tui/board/commands"
import { menuOptions } from "../../../src/tui/board/commands/hints"

// bun's mock.module is process-global; sibling test files mock git/runner + git/merge and those mocks
// leak across files in load order. Establish this file's own git mocks so the merge-dialog tests below
// are deterministic regardless of which file ran first, and drive their behavior through these vars.
let currentBranchValue: string | undefined = "kagan/task"
let localBranches: string[] = ["kagan/task"]
let mergeResult: { ok: boolean; message: string } = { ok: true, message: "Merged" }

mock.module("../../../src/git/runner", () => ({
  bunGitRunner: () => async () => ({ code: 0, stdout: "", stderr: "" }),
  currentBranch: async () => currentBranchValue,
  listLocalBranches: async () => localBranches,
  baseBranchFreshness: async () => ({ ahead: 0 }),
}))

mock.module("../../../src/git/merge", () => ({
  mergeTaskBranch: async () => mergeResult,
}))

let renderSetup: TestRendererSetup | undefined
let intakeTreeSitter: MockTreeSitterClient | undefined

beforeEach(() => {
  currentBranchValue = "kagan/task"
  localBranches = ["kagan/task"]
  mergeResult = { ok: true, message: "Merged" }
  intakeTreeSitter = new MockTreeSitterClient({ autoResolveTimeout: 0 })
  intakeTreeSitter.setMockResult({ highlights: [] })
  configureIntakeMarkdownTreeSitter(intakeTreeSitter)
})

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
  configureIntakeMarkdownTreeSitter(undefined)
  await intakeTreeSitter?.destroy()
  intakeTreeSitter = undefined
})

function mockStore(
  partial: {
    selected: () => string | undefined
    sessions: () => unknown[]
  } & Record<string, unknown>,
): BoardStore {
  return {
    sendBackStopThreshold: 3,
    moveDenyReason: () => undefined,
    notify: () => {},
    selectedSession() {
      const id = partial.selected()
      return partial.sessions().find((item) => (item as { id: string }).id === id) as BoardSession | undefined
    },
    ...partial,
  } as unknown as BoardStore
}

function waitFor(condition: () => boolean, timeoutMs = 500): Promise<void> {
  return new Promise((resolve, reject) => {
    const start = Date.now()
    const interval = setInterval(() => {
      if (condition()) {
        clearInterval(interval)
        resolve()
      } else if (Date.now() - start > timeoutMs) {
        clearInterval(interval)
        reject(new Error("waitFor timeout"))
      }
    }, 10)
  })
}

describe("footerHints", () => {
  function hintSession(kaganStatus: BoardSession["kaganStatus"], metadata: Record<string, unknown> = {}): BoardSession {
    return { id: "s1", kaganStatus, metadata } as unknown as BoardSession
  }

  test("shows the baseline hints when nothing is selected", () => {
    expect(footerHints(undefined, false)).toEqual([
      { key: "j/k/tab", label: "navigate" },
      { key: "enter", label: "menu" },
      { key: "n", label: "new" },
      { key: "/", label: "filter" },
      { key: ",", label: "settings" },
      { key: "?", label: "help" },
      { key: "q", label: "quit" },
    ])
  })

  test("keeps menu-covered actions out of the footer when a card is selected", () => {
    const hints = footerHints(hintSession("backlog"), false)
    expect(hints).toEqual(footerHints(undefined, false))
    expect(hints.map((hint) => hint.key)).not.toContain("m")
    expect(hints.map((hint) => hint.key)).not.toContain("d")
    expect(hints.map((hint) => hint.key)).not.toContain("o")
  })

  test("adds approve/send-back hints only when the selected card is in review", () => {
    const hints = footerHints(hintSession("review", { kagan: {} }), false)
    expect(hints).toContainEqual({ key: "a", label: "approve" })
    expect(hints).toContainEqual({ key: "s", label: "send back" })
  })

  test("omits approve/send-back hints when the selected card is not in review", () => {
    const hints = footerHints(hintSession("backlog"), false)
    expect(hints).not.toContainEqual({ key: "a", label: "approve" })
    expect(hints).not.toContainEqual({ key: "s", label: "send back" })
  })

  test("adds a restart hint for a backlog task with intake history", () => {
    const hints = footerHints(hintSession("backlog", { kagan: { intakeOutcome: "failed" } }), false)
    expect(hints).toContainEqual({ key: "r", label: "restart intake" })
  })

  test("adds a restart hint for a review task with validator history", () => {
    const hints = footerHints(hintSession("review", { kagan: { validatorOutcome: "failed" } }), false)
    expect(hints).toContainEqual({ key: "r", label: "restart review" })
  })

  test("omits the restart hint when nothing has spawned", () => {
    const hints = footerHints(hintSession("backlog", { kagan: {} }), false)
    expect(hints).not.toContainEqual({ key: "r", label: "restart intake" })
  })

  test("adds an esc-clears hint only when a filter is active", () => {
    expect(footerHints(undefined, true)).toContainEqual({ key: "esc", label: "clears it" })
    expect(footerHints(undefined, false)).not.toContainEqual({ key: "esc", label: "clears it" })
  })

  test("adds the update hint only when a release is available", () => {
    expect(footerHints(undefined, false, 0, true)).toContainEqual({ key: "u", label: "update" })
    expect(footerHints(undefined, false)).not.toContainEqual({ key: "u", label: "update" })
  })
})

describe("menuOptions", () => {
  test("leads Review cards with Approve and Send back, and offers intake notes when present", () => {
    const options = menuOptions({
      id: "s1",
      kaganStatus: "review",
      metadata: {
        kagan: {
          intake: { understanding: "Ship the calc", decisions: [], mode: { recommended: "assisted", rationale: "x" } },
        },
      },
    } as unknown as BoardSession)
    expect(options.map((o) => o.value).slice(0, 4)).toEqual(["approve", "send_back", "view", "intake"])
  })

  test("omits intake notes when intake has not run", () => {
    const options = menuOptions({
      id: "s1",
      kaganStatus: "backlog",
      metadata: { kagan: {} },
    } as unknown as BoardSession)
    expect(options.map((o) => o.value)).not.toContain("intake")
    expect(options.map((o) => o.value)[0]).toBe("view")
  })
})

describe("createBoardCommands", () => {
  test("registers every board binding command", () => {
    const names = new Set(
      createBoardCommands({} as TuiPluginApi, {} as BoardStore, () => {}).map((command) => command.name),
    )
    expect(
      BOARD_BINDINGS.filter((binding) => binding.cmd !== "kagan.update").every((binding) => names.has(binding.cmd)),
    ).toBe(true)
  })

  test("kagan.settings opens the settings route", () => {
    let route = ""
    const api = { route: { navigate: (next: string) => (route = next) } } as unknown as TuiPluginApi
    const commands = createBoardCommands(api, {} as BoardStore, () => {})
    commands.find((command) => command.name === "kagan.settings")?.run()
    expect(route).toBe(SETTINGS_ROUTE)
  })

  test("kagan.dismiss clears the filter when help is closed and a filter is active", () => {
    let filter = "board"
    let helpOpen = false
    const store = {
      setFilter: (value: string) => {
        filter = value
      },
      filter: () => filter,
    } as unknown as BoardStore
    const commands = createBoardCommands({} as TuiPluginApi, store, (value) => {
      helpOpen = typeof value === "function" ? value(helpOpen) : value
    })
    commands.find((command) => command.name === "kagan.dismiss")?.run()
    expect(filter).toBe("")
    expect(helpOpen).toBe(false)
  })

  test("kagan.dismiss closes the help overlay instead of clearing the filter when both are active", () => {
    let filter = "board"
    let helpOpen = true
    const store = {
      setFilter: (value: string) => {
        filter = value
      },
      filter: () => filter,
    } as unknown as BoardStore
    const commands = createBoardCommands({} as TuiPluginApi, store, (value) => {
      helpOpen = typeof value === "function" ? value(helpOpen) : value
    })
    commands.find((command) => command.name === "kagan.dismiss")?.run()
    expect(helpOpen).toBe(false)
    expect(filter).toBe("board")
  })

  test("kagan.dismiss does nothing when neither help nor filter is active", () => {
    let filter = ""
    let helpOpen = false
    const store = {
      setFilter: (value: string) => {
        filter = value
      },
      filter: () => filter,
    } as unknown as BoardStore
    const commands = createBoardCommands({} as TuiPluginApi, store, (value) => {
      helpOpen = typeof value === "function" ? value(helpOpen) : value
    })
    commands.find((command) => command.name === "kagan.dismiss")?.run()
    expect(helpOpen).toBe(false)
    expect(filter).toBe("")
  })

  test("kagan.help toggles the help overlay open and closed", () => {
    let helpOpen = false
    const commands = createBoardCommands({} as TuiPluginApi, {} as BoardStore, (value) => {
      helpOpen = typeof value === "function" ? value(helpOpen) : value
    })
    const help = commands.find((command) => command.name === "kagan.help")
    help?.run()
    expect(helpOpen).toBe(true)
    help?.run()
    expect(helpOpen).toBe(false)
  })

  test("kagan.close closes the help overlay when it is open instead of navigating", () => {
    let navigated = false
    const api = { route: { navigate: () => (navigated = true) } } as unknown as TuiPluginApi
    let helpOpen = true
    const commands = createBoardCommands(api, {} as BoardStore, (value) => {
      helpOpen = typeof value === "function" ? value(helpOpen) : value
    })
    commands.find((command) => command.name === "kagan.close")?.run()
    expect(helpOpen).toBe(false)
    expect(navigated).toBe(false)
  })

  test("kagan.close navigates home when the help overlay is already closed", () => {
    let navigated = false
    const api = { route: { navigate: () => (navigated = true) } } as unknown as TuiPluginApi
    let helpOpen = false
    const commands = createBoardCommands(api, {} as BoardStore, (value) => {
      helpOpen = typeof value === "function" ? value(helpOpen) : value
    })
    commands.find((command) => command.name === "kagan.close")?.run()
    expect(navigated).toBe(true)
  })

  test("kagan.move_next opens the intake dialog instead of moving when a required decision is pending", async () => {
    const session = {
      id: "s1",
      kaganStatus: "backlog" as const,
      metadata: {
        kagan: {
          intakeOutcome: "ran",
          intake: {
            understanding: "Adds a retry wrapper.",
            decisions: [{ id: "d1", question: "Max retries?", assumption: "3", required: true }],
          },
        },
      },
    }
    let moveNextCalls = 0
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      moveNext: async () => {
        moveNextCalls++
      },
    })
    let dialogRendered: (() => unknown) | undefined
    const api = {
      ui: { dialog: { replace: (render: () => unknown) => (dialogRendered = render) } },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    expect(dialogRendered).toBeDefined()
    expect(moveNextCalls).toBe(0)
  })

  test("kagan.move_next notifies instead of moving while intake has not finished running", async () => {
    const session = { id: "s1", kaganStatus: "backlog" as const, metadata: { kagan: {} } }
    let moveNextCalls = 0
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      moveNext: async () => {
        moveNextCalls++
      },
      notify: (options: unknown) => notices.push(options),
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    expect(moveNextCalls).toBe(0)
    expect(notices).toContainEqual({ variant: "warning", title: "Kagan", message: "Intake is still being prepared" })
  })

  test("kagan.move_next moves normally once a backlog task is intake-ready", async () => {
    const session = { id: "s1", kaganStatus: "backlog" as const, metadata: { kagan: { intakeOutcome: "ran" } } }
    let moveNextCalls = 0
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      moveNext: async () => {
        moveNextCalls++
      },
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    expect(moveNextCalls).toBe(1)
  })

  test("kagan.move_next surfaces an advisory confirm before starting a non-autonomous task", async () => {
    const session = {
      id: "s1",
      title: "Task",
      slug: "task",
      kaganStatus: "backlog" as const,
      metadata: {
        kagan: {
          intakeOutcome: "ran",
          taskNumber: 1,
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
          },
        },
      },
    }
    let moveNextCalls = 0
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      checkCommand: "bun run verify",
      moveNext: async () => {
        moveNextCalls++
      },
    })
    let dialogRendered: (() => JSX.Element) | undefined
    const api = mockTuiApi({
      renderer: { width: 60, height: 28 },
      ui: {
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => {
            dialogRendered = render as () => JSX.Element
          },
        },
      },
    })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    expect(dialogRendered).toBeDefined()
    expect(moveNextCalls).toBe(0)
    renderSetup = await testRender(dialogRendered!, { width: 60, height: 28, kittyKeyboard: true })
    attachRendererMockInput(api, renderSetup)
    await renderSetup.flush()
    await waitFor(() => renderSetup!.captureCharFrame().includes("No trusted check and the blast radius is high."))
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("Assisted mode")
    expect(frame).toContain("Start agent anyway")
    renderSetup.mockInput.pressEnter()
    await waitFor(() => moveNextCalls === 1)
  })

  test("kagan.move_next intake decision chain advances through multiple decisions and enforces a substantive override answer (R5.4)", async () => {
    const session = {
      id: "s1",
      title: "Retry",
      slug: "retry",
      kaganStatus: "backlog" as const,
      metadata: {
        kagan: {
          intakeOutcome: "ran",
          worktree: "/wt",
          taskNumber: 1,
          intake: {
            understanding: "Adds a retry wrapper.",
            decisions: [
              { id: "d1", question: "Max retries?", assumption: "3", required: true },
              { id: "d2", question: "Backoff strategy?", assumption: "linear", required: true },
            ],
          },
        },
      } as Record<string, unknown>,
    }
    const notices: unknown[] = []
    let moveNextCalls = 0
    let updateCalls = 0
    let currentRender: (() => JSX.Element) | undefined
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
      moveNext: async () => {
        moveNextCalls++
      },
    })
    const api = mockTuiApi({
      renderer: { width: 60, height: 30 },
      client: {
        session: {
          get: async () => ({ data: { metadata: session.metadata } }),
          update: async (parameters: { metadata: Record<string, unknown> }) => {
            updateCalls++
            session.metadata = parameters.metadata
          },
        },
      },
      ui: {
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => {
            currentRender = render as () => JSX.Element
          },
        },
      },
    })

    const remount = async () => {
      if (!currentRender) throw new Error("expected dialog render")
      if (renderSetup) {
        await renderSetup.renderer.destroy()
        renderSetup = undefined
      }
      renderSetup = await testRender(currentRender, { width: 60, height: 30, kittyKeyboard: true })
      attachRendererMockInput(api, renderSetup)
      await renderSetup.flush()
      await Bun.sleep(40)
      intakeTreeSitter?.resolveAllHighlightOnce()
      await renderSetup.flush()
    }

    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    await remount()
    expect(renderSetup!.captureCharFrame()).toContain("Intake decision (1/2)")
    expect(renderSetup!.captureCharFrame()).toContain("Max retries?")
    renderSetup!.mockInput.pressEnter()
    await waitFor(() => updateCalls === 1)
    await remount()
    expect(renderSetup!.captureCharFrame()).toContain("Intake decision (1/1)")
    expect(renderSetup!.captureCharFrame()).toContain("Backoff strategy?")
    renderSetup!.mockInput.pressKey("j")
    await renderSetup!.flush()
    renderSetup!.mockInput.pressEnter()
    await remount()
    expect(renderSetup!.captureCharFrame()).toContain("Your answer")
    expect(renderSetup!.captureCharFrame()).toContain("Backoff strategy?")

    const updatesBeforeOverride = updateCalls
    await renderSetup!.mockInput.typeText(
      "Use exponential backoff to avoid overwhelming the dependency during retries.",
    )
    renderSetup!.mockInput.pressEnter()
    await waitFor(() => moveNextCalls === 1)
    expect(updateCalls).toBe(updatesBeforeOverride + 1)
    const kagan = (session.metadata as { kagan: { intake: { decisions: Record<string, unknown>[] } } }).kagan
    expect(kagan.intake.decisions[1]).toMatchObject({
      resolution: "overridden",
      answer: "Use exponential backoff to avoid overwhelming the dependency during retries.",
    })
  })

  test("kagan.move_next intake override leaves the prompt open and does not resolve on a placeholder answer", async () => {
    const session = {
      id: "s1",
      title: "Retry",
      slug: "retry",
      kaganStatus: "backlog" as const,
      metadata: {
        kagan: {
          intakeOutcome: "ran",
          worktree: "/wt",
          intake: {
            understanding: "Adds a retry wrapper.",
            decisions: [{ id: "d1", question: "Max retries?", assumption: "3", required: true }],
          },
        },
      } as Record<string, unknown>,
    }
    const notices: unknown[] = []
    let updateCalls = 0
    let currentRender: (() => JSX.Element) | undefined
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
    })
    const api = mockTuiApi({
      renderer: { width: 60, height: 30 },
      client: {
        session: {
          get: async () => ({ data: { metadata: session.metadata } }),
          update: async (parameters: { metadata: Record<string, unknown> }) => {
            updateCalls++
            session.metadata = parameters.metadata
          },
        },
      },
      ui: {
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => {
            currentRender = render as () => JSX.Element
          },
        },
      },
    })

    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    renderSetup = await testRender(currentRender!, { width: 60, height: 30, kittyKeyboard: true })
    attachRendererMockInput(api, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("j")
    await renderSetup.flush()
    renderSetup.mockInput.pressEnter()
    if (renderSetup) {
      await renderSetup.renderer.destroy()
      renderSetup = undefined
    }
    renderSetup = await testRender(currentRender!, { width: 60, height: 30, kittyKeyboard: true })
    attachRendererMockInput(api, renderSetup)
    await renderSetup.flush()
    expect(renderSetup.captureCharFrame()).toContain("Your answer")

    await renderSetup.mockInput.typeText("ok")
    renderSetup.mockInput.pressEnter()
    await renderSetup.flush()
    expect(notices).toEqual([
      { variant: "warning", title: "Kagan", message: "Add a substantive answer to override this assumption" },
    ])
    expect(updateCalls).toBe(0)
    expect(renderSetup.captureCharFrame()).toContain("Your answer")
  })

  test("kagan.approve opens the findings review popup when the review is clean and approvable", async () => {
    const session = {
      id: "s1",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran" } },
    }
    const store = mockStore({ selected: () => "s1", sessions: () => [session] })
    let dialogRendered: (() => unknown) | undefined
    const api = {
      state: { vcs: { branch: "main" }, path: { worktree: "/repo" } },
      ui: { dialog: { replace: (render: () => unknown) => (dialogRendered = render) } },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.approve")
      ?.run()
    expect(dialogRendered).toBeDefined()
  })

  test("kagan.approve warns and opens no popup when the selected card is not in review", async () => {
    const session = { id: "s1", kaganStatus: "backlog" as const, metadata: { kagan: {} } }
    const notices: unknown[] = []
    let dialogRendered: (() => unknown) | undefined
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    const api = {
      ui: { dialog: { replace: (render: () => unknown) => (dialogRendered = render) } },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.approve")
      ?.run()
    expect(dialogRendered).toBeUndefined()
    expect(notices).toContainEqual({
      variant: "warning",
      title: "Kagan",
      message: "Approve only applies to tasks in review",
    })
  })

  test("kagan.approve does nothing when no card is selected", async () => {
    const store = mockStore({ selected: () => undefined, sessions: () => [] })
    let dialogRendered: (() => unknown) | undefined
    const api = {
      ui: { dialog: { replace: (render: () => unknown) => (dialogRendered = render) } },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.approve")
      ?.run()
    expect(dialogRendered).toBeUndefined()
  })

  test("kagan.approve passes store.checkCommand to the findings review dialog", async () => {
    const session = {
      id: "s1",
      kaganStatus: "review" as const,
      metadata: {
        kagan: {
          boardTask: true,
          validatorOutcome: "ran",
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
          },
        },
      },
    }
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      checkCommand: "bun run verify",
    })
    let dialogRendered: (() => unknown) | undefined
    const api = mockTuiApi({
      theme: {
        current: {
          text: "white",
          textMuted: "gray",
          primary: "magenta",
          selectedListItemText: "cyan",
          success: "green",
          warning: "yellow",
          error: "red",
          accent: "magenta",
        },
      },
      ui: {
        dialog: {
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => (dialogRendered = render),
        },
      },
    } as unknown as Partial<TuiPluginApi>)
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.approve")
      ?.run()
    expect(dialogRendered).toBeDefined()
    renderSetup = await testRender(dialogRendered as () => JSX.Element, { width: 80, height: 12 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame.replace(/\s+/g, " ")).toContain("No trusted check and the blast radius is high.")
    expect(frame.replace(/\s+/g, " ")).not.toContain("no automatic check configured")
  })

  test("kagan.send_back warns when the selected card is not in review", async () => {
    const session = { id: "s1", kaganStatus: "backlog" as const, metadata: { kagan: {} } }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    expect(notices).toContainEqual({
      variant: "warning",
      title: "Kagan",
      message: "Send back only applies to tasks in review",
    })
  })

  test("kagan.send_back warns and does not spawn an iteration when In Progress is at cap", async () => {
    const session = { id: "s1", kaganStatus: "review" as const, metadata: { kagan: { boardTask: true } } }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      moveDenyReason: () => "In Progress WIP limit of 2 reached",
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    expect(notices).toEqual([{ variant: "warning", title: "Kagan", message: "In Progress WIP limit of 2 reached" }])
  })

  test("kagan.send_back sends back directly when below the stop threshold", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, generation: 2, worktree: "/tmp" } },
    }
    let dialogRendered: (() => unknown) | undefined
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = {
      client,
      ui: { dialog: { replace: (render: () => unknown) => (dialogRendered = render), clear: () => {} } },
    } as unknown as TuiPluginApi
    const store = mockStore({ selected: () => "s1", sessions: () => [session], refresh: async () => {} })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    expect(dialogRendered).toBeUndefined()
    expect(capture.createCalls).toBe(1)
  })

  type SendBackDialogOption = "send_back" | "take_over" | "leave"
  type SendBackDialogProps = {
    title: string
    options: { title: string; value: SendBackDialogOption }[]
    onSelect: (option: { value: SendBackDialogOption }) => void
  }

  test("kagan.send_back opens a three-way stop dialog at the threshold", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, generation: 3, worktree: "/tmp" } },
    }
    let dialogRendered: (() => SendBackDialogProps) | undefined
    const { client } = mockSessionClient({ metadata: session.metadata })
    const api = {
      client: { ...client, tui: { selectSession: async () => {} } },
      ui: {
        dialog: {
          replace: (render: () => SendBackDialogProps) => (dialogRendered = render),
          clear: () => {},
        },
        DialogSelect: (props: SendBackDialogProps) => props,
      },
    } as unknown as TuiPluginApi
    const store = mockStore({ selected: () => "s1", sessions: () => [session], refresh: async () => {} })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    expect(dialogRendered).toBeDefined()
    const props = dialogRendered!()
    expect(props.title).toContain("3")
    expect(props.options.map((option) => option.value)).toEqual(["send_back", "take_over", "leave"])
  })

  test("kagan.send_back stop dialog can iterate again", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, generation: 3, worktree: "/tmp" } },
    }
    let dialogRendered: (() => SendBackDialogProps) | undefined
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = {
      client: { ...client, tui: { selectSession: async () => {} } },
      ui: {
        dialog: {
          replace: (render: () => SendBackDialogProps) => (dialogRendered = render),
          clear: () => {},
        },
        DialogSelect: (props: SendBackDialogProps) => props,
      },
    } as unknown as TuiPluginApi
    const store = mockStore({ selected: () => "s1", sessions: () => [session], refresh: async () => {} })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    dialogRendered!().onSelect({ value: "send_back" })
    await waitFor(() => capture.createCalls === 1)
    expect(capture.createCalls).toBe(1)
  })

  test("kagan.send_back stop dialog can hand off to the human", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, generation: 3, worktree: "/tmp" } },
    }
    let dialogRendered: (() => SendBackDialogProps) | undefined
    let selectedSessionID: string | undefined
    const { client } = mockSessionClient({ metadata: session.metadata })
    const api = {
      client: {
        ...client,
        tui: {
          selectSession: async (parameters: { sessionID?: string }) => {
            selectedSessionID = parameters.sessionID
          },
        },
      },
      ui: {
        dialog: {
          replace: (render: () => SendBackDialogProps) => (dialogRendered = render),
          clear: () => {},
        },
        DialogSelect: (props: SendBackDialogProps) => props,
      },
    } as unknown as TuiPluginApi
    const store = mockStore({ selected: () => "s1", sessions: () => [session], refresh: async () => {} })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    dialogRendered!().onSelect({ value: "take_over" })
    expect(selectedSessionID).toBe("s1")
  })

  test("kagan.send_back stop dialog can leave the task in review", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, generation: 3, worktree: "/tmp" } },
    }
    let clearCalls = 0
    let dialogRendered: (() => SendBackDialogProps) | undefined
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = {
      client: { ...client, tui: { selectSession: async () => {} } },
      ui: {
        dialog: {
          replace: (render: () => SendBackDialogProps) => (dialogRendered = render),
          clear: () => {
            clearCalls++
          },
        },
        DialogSelect: (props: SendBackDialogProps) => props,
      },
    } as unknown as TuiPluginApi
    const store = mockStore({ selected: () => "s1", sessions: () => [session], refresh: async () => {} })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.send_back")
      ?.run()
    dialogRendered!().onSelect({ value: "leave" })
    expect(clearCalls).toBe(1)
    expect(capture.createCalls).toBe(0)
  })

  test("kagan.move_prev moves directly when the selected card is not in review", async () => {
    const session = { id: "s1", kaganStatus: "in_progress" as const, metadata: { kagan: {} } }
    let movePreviousCalls = 0
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      movePrevious: async () => {
        movePreviousCalls++
      },
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.move_prev")
      ?.run()
    expect(movePreviousCalls).toBe(1)
  })

  test("kagan.move_prev routes a review card through send-back instead of moving directly", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, generation: 1, worktree: "/tmp" } },
    }
    let movePreviousCalls = 0
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = { client } as unknown as TuiPluginApi
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      refresh: async () => {},
      movePrevious: async () => {
        movePreviousCalls++
      },
    })
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_prev")
      ?.run()
    expect(movePreviousCalls).toBe(0)
    expect(capture.createCalls).toBe(1)
  })

  test("kagan.retry warns when nothing has spawned", async () => {
    const session = { id: "s1", kaganStatus: "backlog" as const, metadata: { kagan: {} } }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.retry")
      ?.run()
    expect(notices).toContainEqual({ variant: "warning", title: "Kagan", message: "Nothing to restart" })
  })

  test("kagan.retry clears a failed intake and notifies success", async () => {
    const session = {
      id: "s1",
      kaganStatus: "backlog" as const,
      metadata: { kagan: { boardTask: true, intakeOutcome: "failed" } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
    })
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = { client } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.retry")
      ?.run()
    const kagan = (capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({ intakeSessionID: undefined, intakeOutcome: undefined, intakeAttempts: 0 })
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Restarting intake" })
  })

  test("kagan.retry clears a validator helperError and notifies success even before the outcome flips to failed", async () => {
    const session = {
      id: "s1",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, helperError: { role: "validator", message: "boom" } } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
    })
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = { client } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.retry")
      ?.run()
    const kagan = (capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({ validatorSessionID: undefined, validatorOutcome: undefined, validatorAttempts: 0 })
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Restarting review" })
  })

  test("kagan.retry recovers a validator stuck running with no outcome and no failure event", async () => {
    const session = {
      id: "s1",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorSessionID: "v1" } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
    })
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = { client } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.retry")
      ?.run()
    const kagan = (capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({ validatorSessionID: undefined, validatorOutcome: undefined, validatorAttempts: 0 })
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Restarting review" })
  })

  test("kagan.retry restarts a validator that already ran and is awaiting triage", async () => {
    const session = {
      id: "s1",
      kaganStatus: "review" as const,
      metadata: {
        kagan: {
          boardTask: true,
          validatorSessionID: "v1",
          validatorOutcome: "ran",
          findings: [{ id: "f1", summary: "issue" }],
          approved: true,
        },
      },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
    })
    const { client, capture } = mockSessionClient({ metadata: session.metadata })
    const api = { client } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.retry")
      ?.run()
    const kagan = (capture.updateArg!.metadata as { kagan: Record<string, unknown> }).kagan
    expect(kagan).toMatchObject({
      validatorSessionID: undefined,
      validatorOutcome: undefined,
      findings: undefined,
      approved: undefined,
    })
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Restarting review" })
  })

  test("kagan.open_session selects and focuses the selected session", async () => {
    let selectedSessionID: string | undefined
    const store = { selected: () => "s1" } as unknown as BoardStore
    const api = {
      client: {
        tui: {
          selectSession: async (parameters: { sessionID?: string }) => {
            selectedSessionID = parameters.sessionID
          },
        },
      },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.open_session")
      ?.run()
    expect(selectedSessionID).toBe("s1")
  })

  test("kagan.menu does nothing when no task is selected", () => {
    let dialogOpened = false
    const store = mockStore({ selected: () => undefined, sessions: () => [] })
    const api = { ui: { dialog: { replace: () => (dialogOpened = true) } } } as unknown as TuiPluginApi
    createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.menu")
      ?.run()
    expect(dialogOpened).toBe(false)
  })

  test("kagan.menu lists the actions that apply to the selected card's status", () => {
    const session = {
      id: "s1",
      title: "Add retry",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "failed" } },
    }
    const store = mockStore({ selected: () => "s1", sessions: () => [session] })
    let dialogRendered: (() => { options: { value: string }[] }) | undefined
    const api = {
      ui: {
        dialog: {
          replace: (render: () => unknown) => {
            dialogRendered = render as () => { options: { value: string }[] }
          },
        },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.menu")
      ?.run()
    expect(dialogRendered!().options.map((option) => option.value)).toEqual([
      "approve",
      "send_back",
      "view",
      "open",
      "advance",
      "retry",
      "delete",
    ])
  })

  test("kagan.menu 'Open session' clears the menu and focuses the session", async () => {
    const session = { id: "s1", kaganStatus: "backlog" as const, metadata: { kagan: { boardTask: true } } }
    let selectedSessionID: string | undefined
    let cleared = false
    const store = mockStore({ selected: () => "s1", sessions: () => [session] })
    let dialogRendered:
      | (() => { options: { value: string }[]; onSelect: (option: { value: string }) => void })
      | undefined
    const api = {
      client: {
        tui: {
          selectSession: async (parameters: { sessionID?: string }) => {
            selectedSessionID = parameters.sessionID
          },
        },
      },
      ui: {
        dialog: {
          replace: (render: () => unknown) => {
            dialogRendered = render as () => {
              options: { value: string }[]
              onSelect: (option: { value: string }) => void
            }
          },
          clear: () => (cleared = true),
        },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.menu")
      ?.run()
    const menuProps = dialogRendered!()
    menuProps.onSelect(menuProps.options.find((option) => option.value === "open")!)
    await waitFor(() => selectedSessionID === "s1")
    expect(cleared).toBe(true)
  })

  test("kagan.menu 'View details' peeks the live session titled with its task number and title", async () => {
    const session = {
      id: "s1",
      title: "Add retry",
      kaganStatus: "backlog" as const,
      metadata: { kagan: { boardTask: true, taskNumber: 4 } },
    }
    const store = mockStore({ selected: () => "s1", sessions: () => [session] })
    const renders: Array<() => JSX.Element> = []
    const api = {
      theme: { current: mockTheme },
      renderer: { width: 100, height: 30, on: () => {}, off: () => {} },
      ui: {
        dialog: {
          replace: (render: () => JSX.Element) => renders.push(render),
          clear: () => {},
          setSize: () => {},
        },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.menu")
      ?.run()
    const menuProps = renders[0]!() as unknown as {
      options: { value: string }[]
      onSelect: (option: { value: string }) => void
    }
    menuProps.onSelect(menuProps.options.find((option) => option.value === "view")!)
    await waitFor(() => renders.length === 2)
    renderSetup = await testRender(renders[1]!, { width: 100, height: 30 })
    await renderSetup.flush()
    const frame = renderSetup.captureCharFrame()
    expect(frame).toContain("#4 Add retry")
    expect(frame).toContain("↑↓ scroll")
  })

  test("kagan.menu 'Archive' archives a done task and refreshes the board", async () => {
    const session = { id: "s1", kaganStatus: "done" as const, metadata: { kagan: { boardTask: true, approved: true } } }
    let updateArg: { sessionID?: string; time?: { archived?: number } } | undefined
    let refreshCalls = 0
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {
        refreshCalls++
      },
    })
    const renders: Array<() => unknown> = []
    const api = {
      client: {
        session: {
          update: async (parameters: unknown) => {
            updateArg = parameters as typeof updateArg
          },
        },
      },
      ui: {
        dialog: { replace: (render: () => unknown) => renders.push(render), clear: () => {} },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.menu")
      ?.run()
    const menuProps = renders[0]!() as { options: { value: string }[]; onSelect: (option: { value: string }) => void }
    menuProps.onSelect(menuProps.options.find((option) => option.value === "archive")!)
    await waitFor(() => refreshCalls === 1)
    expect(updateArg?.sessionID).toBe("s1")
    expect(updateArg?.time?.archived).toBeGreaterThan(0)
    expect(notices).toContainEqual({
      variant: "success",
      title: "Kagan",
      message: "Archived — still available in the session list",
    })
  })

  async function driveApproveToMergeDialog(
    api: TuiPluginApi,
    store: BoardStore,
    renders: Array<() => unknown>,
  ): Promise<{
    options: { title: string; value: string }[]
    onSelect: (option: { value: string }) => Promise<void> | void
  }> {
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.approve")
      ?.run()
    renderSetup = await testRender(renders.at(-1) as () => JSX.Element, { width: 100, height: 24 })
    attachRendererMockInput(api, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("a")
    const asMergeDialog = (render: (() => unknown) | undefined) => {
      if (!render) return undefined
      try {
        const props = render() as { options?: { title: string; value: string }[]; onSelect?: unknown }
        return typeof props.onSelect === "function" && Array.isArray(props.options) ? props : undefined
      } catch {
        return undefined
      }
    }
    // The findings dialog's approve handler runs fire-and-forget and may re-render before it opens the
    // merge dialog, so wait for the merge DialogSelect itself rather than a fixed render count.
    const latestMergeDialog = () => {
      let found: ReturnType<typeof asMergeDialog>
      for (const render of renders) {
        const props = asMergeDialog(render)
        if (props) found = props
      }
      return found
    }
    await waitFor(() => latestMergeDialog() !== undefined)
    return latestMergeDialog() as {
      options: { title: string; value: string }[]
      onSelect: (option: { value: string }) => Promise<void> | void
    }
  }

  test("approving reaches promptAnotherBranch, which warns when the task's own branch is the only local branch", async () => {
    // The only local branch is the task's own branch, so filtering it out leaves nothing to merge into.
    currentBranchValue = "kagan/task"
    localBranches = ["kagan/task"]
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran", worktree: "/task", baseBranch: "main" } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    const renders: Array<() => unknown> = []
    const api = mockTuiApi({
      state: { path: { worktree: "/main" }, vcs: { branch: "main" } },
      ui: {
        toast: () => {},
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => renders.push(render),
        },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as Partial<TuiPluginApi>)

    const mergeProps = await driveApproveToMergeDialog(api, store, renders)
    await mergeProps.onSelect({ value: "another" })
    expect(notices).toContainEqual({
      variant: "warning",
      title: "Kagan",
      message: "No other local branches to merge into",
    })
  })

  test("approving and merging into the current branch surfaces the merge failure without approving", async () => {
    // The merge into the current branch reports a conflict.
    mergeResult = { ok: false, message: "CONFLICT (content): Merge conflict in shared.txt" }
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran", worktree: "/task", baseBranch: "main" } },
    }
    const notices: unknown[] = []
    let refreshCalls = 0
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {
        refreshCalls++
      },
      moveTo: async () => {},
    })
    const renders: Array<() => unknown> = []
    const api = mockTuiApi({
      state: { path: { worktree: "/main" }, vcs: { branch: "main" } },
      ui: {
        toast: () => {},
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => renders.push(render),
        },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as Partial<TuiPluginApi>)

    const mergeProps = await driveApproveToMergeDialog(api, store, renders)
    await mergeProps.onSelect({ value: "current" })
    // Other notices may accompany the merge result, so match the conflict notice specifically.
    const mergeFailure = notices.find((n) => /conflict/i.test((n as { message?: string }).message ?? ""))
    expect(mergeFailure).toMatchObject({ variant: "error", title: "Kagan" })
    expect(refreshCalls).toBe(0)
  })

  test("approving and declining to merge approves, refreshes, moves to done, then notifies success, in that order", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran" } },
    }
    const sequence: string[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => sequence.push(`notify:${(options as { message: string }).message}`),
      refresh: async () => {
        sequence.push("refresh")
      },
      moveTo: async () => {
        sequence.push("moveTo:done")
      },
    })
    const { client } = mockSessionClient({
      metadata: session.metadata,
      update: () => {
        sequence.push("approve")
      },
    })
    const renders: Array<() => unknown> = []
    const api = mockTuiApi({
      client,
      state: { path: { worktree: "/repo" }, vcs: { branch: "main" } },
      ui: {
        toast: () => {},
        dialog: {
          open: true,
          setSize: () => {},
          clear: () => {},
          replace: (render: () => unknown) => renders.push(render),
        },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as Partial<TuiPluginApi>)

    const mergeProps = await driveApproveToMergeDialog(api, store, renders)
    await mergeProps.onSelect({ value: "none" })
    expect(sequence).toEqual(["approve", "refresh", "moveTo:done", "notify:Task approved"])
  })
})
