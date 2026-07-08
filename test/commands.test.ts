/** @jsxImportSource @opentui/solid */
import { afterEach, describe, expect, test } from "bun:test"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { TestRendererSetup } from "@opentui/core/testing"
import { testRender } from "@opentui/solid"
import type { JSX } from "solid-js"
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises"
import { tmpdir } from "node:os"
import { join } from "node:path"
import type { BoardSession } from "../src/types"
import type { BoardStore, MergeDialogHandlers } from "../src/commands"
import { bunGitRunner } from "../src/git"
import { isTrustPacket, serializeTrustPacket } from "../src/trust-packet"
import { attachRendererMockInput, mockSessionClient, mockTheme, mockTuiApi } from "./fixtures/api"

const { createBoardCommands, footerHints, menuOptions, mergeChoiceOptions, openMergeDialog } = await import(
  "../src/commands"
)

let renderSetup: TestRendererSetup | undefined
const tempDirs: string[] = []

afterEach(async () => {
  await renderSetup?.renderer.destroy()
  renderSetup = undefined
  await Promise.all(tempDirs.splice(0).map((dir) => rm(dir, { recursive: true, force: true })))
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

describe("mergeChoiceOptions", () => {
  test("labels the current-branch and another-branch choices as squash-merge when squash is on", () => {
    expect(mergeChoiceOptions("main", true)).toEqual([
      { title: "Squash-merge into main", value: "current" },
      { title: "Squash-merge into another branch…", value: "another" },
      { title: "No action", value: "none" },
    ])
  })

  test("labels the choices as a plain merge when squash is off", () => {
    expect(mergeChoiceOptions("main", false)).toEqual([
      { title: "Merge into main", value: "current" },
      { title: "Merge into another branch…", value: "another" },
      { title: "No action", value: "none" },
    ])
  })

  test("omits the current-branch choice when there is no current branch", () => {
    expect(mergeChoiceOptions(undefined, true)).toEqual([
      { title: "Squash-merge into another branch…", value: "another" },
      { title: "No action", value: "none" },
    ])
  })
})

describe("footerHints", () => {
  function hintSession(kaganStatus: BoardSession["kaganStatus"], metadata: Record<string, unknown> = {}): BoardSession {
    return { id: "s1", kaganStatus, metadata } as unknown as BoardSession
  }

  test("shows the baseline hints when nothing is selected", () => {
    expect(footerHints(undefined, false)).toEqual([
      { key: "j/k/h/l", label: "navigate" },
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

  test("adds a retry hint for a retryable backlog intake", () => {
    const hints = footerHints(hintSession("backlog", { kagan: { intakeOutcome: "failed" } }), false)
    expect(hints).toContainEqual({ key: "r", label: "retry" })
  })

  test("adds a retry hint for a retryable review validator", () => {
    const hints = footerHints(hintSession("review", { kagan: { validatorOutcome: "failed" } }), false)
    expect(hints).toContainEqual({ key: "r", label: "retry" })
  })

  test("omits the retry hint when nothing is retryable", () => {
    const hints = footerHints(hintSession("backlog", { kagan: {} }), false)
    expect(hints).not.toContainEqual({ key: "r", label: "retry" })
  })

  test("adds an esc-clears hint only when a filter is active", () => {
    expect(footerHints(undefined, true)).toContainEqual({ key: "esc", label: "clears it" })
    expect(footerHints(undefined, false)).not.toContainEqual({ key: "esc", label: "clears it" })
  })
})

describe("menuOptions", () => {
  function menuSession(kaganStatus: BoardSession["kaganStatus"], metadata: Record<string, unknown> = {}): BoardSession {
    return { id: "s1", title: "Task", kaganStatus, metadata } as unknown as BoardSession
  }

  test("backlog: view, open, advance, export, import, delete — no review-only or archive actions", () => {
    const options = menuOptions(menuSession("backlog"))
    expect(options.map((option) => option.value)).toEqual(["view", "open", "advance", "export", "import", "delete"])
  })

  test("backlog: adds retry once the intake helper is retryable", () => {
    const options = menuOptions(menuSession("backlog", { kagan: { intakeOutcome: "failed" } }))
    expect(options.map((option) => option.value)).toContain("retry")
  })

  test("review: adds send back, approve, and retry once the validator is retryable", () => {
    const options = menuOptions(menuSession("review", { kagan: { validatorOutcome: "failed" } }))
    expect(options.map((option) => option.value)).toEqual([
      "view",
      "open",
      "advance",
      "send_back",
      "approve",
      "retry",
      "export",
      "import",
      "delete",
    ])
  })

  test("done: no advance, send back, approve, or retry — adds archive", () => {
    const options = menuOptions(menuSession("done"))
    expect(options.map((option) => option.value)).toEqual(["view", "open", "export", "import", "archive", "delete"])
  })

  test("titles carry the direct shortcut for keyed actions", () => {
    const options = menuOptions(menuSession("review", { kagan: { validatorOutcome: "failed" } }))
    expect(options.find((option) => option.value === "open")?.title).toBe("Open session — o")
    expect(options.find((option) => option.value === "advance")?.title).toBe("Advance — m")
    expect(options.find((option) => option.value === "send_back")?.title).toBe("Send back — s")
    expect(options.find((option) => option.value === "approve")?.title).toBe("Approve — a")
    expect(options.find((option) => option.value === "retry")?.title).toBe("Retry intake/review — r")
    expect(options.find((option) => option.value === "delete")?.title).toBe("Delete — d")
    expect(options.find((option) => option.value === "view")?.title).toBe("View details")
  })
})

describe("createBoardCommands", () => {
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
      kaganStatus: "backlog" as const,
      metadata: {
        kagan: {
          intakeOutcome: "ran",
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
      checkCommand: "bun run check",
      moveNext: async () => {
        moveNextCalls++
      },
    })
    let dialogRendered: (() => { title: string; message: string; onConfirm: () => Promise<void> }) | undefined
    const api = {
      ui: {
        DialogConfirm: (props: { title: string; message: string; onConfirm: () => Promise<void> }) => props,
        dialog: {
          replace: (render: () => { title: string; message: string; onConfirm: () => Promise<void> }) =>
            (dialogRendered = render),
          clear: () => {},
        },
      },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    expect(dialogRendered).toBeDefined()
    expect(moveNextCalls).toBe(0)
    const props = dialogRendered!()
    expect(props.title).toBe("This one looks better driven by you")
    expect(props.message).toBe("No trusted check and the blast radius is high. Start the agent on it anyway?")
    await props.onConfirm()
    expect(moveNextCalls).toBe(1)
  })

  type DecisionSelectProps = {
    title: string
    options: { title: string; value: string; description?: string }[]
    onSelect: (option: { value: string }) => void
  }
  type AnswerPromptProps = {
    title: string
    placeholder: string
    onConfirm: (answer: string) => Promise<void>
    onCancel: () => void
  }

  test("kagan.move_next intake decision chain advances through multiple decisions and enforces a substantive override answer (R5.4)", async () => {
    const session = {
      id: "s1",
      kaganStatus: "backlog" as const,
      metadata: {
        kagan: {
          intakeOutcome: "ran",
          worktree: "/wt",
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
    let clearCalls = 0
    let latest: (() => DecisionSelectProps | AnswerPromptProps) | undefined
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
      moveNext: async () => {
        moveNextCalls++
      },
    })
    const api = {
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
          replace: (render: () => unknown) => {
            latest = render as () => DecisionSelectProps | AnswerPromptProps
          },
          clear: () => {
            clearCalls++
          },
        },
        DialogSelect: (props: unknown) => props,
        DialogPrompt: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi

    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()

    let selectProps = latest!() as DecisionSelectProps
    expect(selectProps.title).toBe("Intake decision (1/2)")
    selectProps.onSelect({ value: "approved" })
    await waitFor(() => (latest!() as DecisionSelectProps).title === "Intake decision (1/1)")

    selectProps = latest!() as DecisionSelectProps
    expect(selectProps.options.map((option) => option.value)).toEqual(["approved", "overridden"])
    selectProps.onSelect({ value: "overridden" })
    let promptProps = latest!() as AnswerPromptProps
    expect(promptProps.title).toBe("Your answer")

    const clearsBeforeOverride = clearCalls
    const updatesBeforeOverride = updateCalls
    await promptProps.onConfirm("ok")
    expect(notices).toContainEqual({
      variant: "warning",
      title: "Kagan",
      message: "Add a substantive answer to override this assumption",
    })
    expect(clearCalls).toBe(clearsBeforeOverride)
    expect(updateCalls).toBe(updatesBeforeOverride)
    promptProps = latest!() as AnswerPromptProps
    expect(promptProps.title).toBe("Your answer")

    await promptProps.onConfirm("Use exponential backoff to avoid overwhelming the dependency during retries.")
    expect(clearCalls).toBe(clearsBeforeOverride + 1)
    expect(updateCalls).toBe(updatesBeforeOverride + 1)
    await waitFor(() => moveNextCalls === 1)
    const kagan = (session.metadata as { kagan: { intake: { decisions: Record<string, unknown>[] } } }).kagan
    expect(kagan.intake.decisions[1]).toMatchObject({
      resolution: "overridden",
      answer: "Use exponential backoff to avoid overwhelming the dependency during retries.",
    })
  })

  test("kagan.move_next intake override leaves the prompt open and does not resolve on a placeholder answer", async () => {
    const session = {
      id: "s1",
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
    let clearCalls = 0
    let latest: (() => DecisionSelectProps | AnswerPromptProps) | undefined
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
      refresh: async () => {},
    })
    const api = {
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
          replace: (render: () => unknown) => {
            latest = render as () => DecisionSelectProps | AnswerPromptProps
          },
          clear: () => {
            clearCalls++
          },
        },
        DialogSelect: (props: unknown) => props,
        DialogPrompt: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi

    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.move_next")
      ?.run()
    ;(latest!() as DecisionSelectProps).onSelect({ value: "overridden" })
    const promptRender = latest
    const promptProps = latest!() as AnswerPromptProps

    await promptProps.onConfirm("ok")
    expect(notices).toEqual([
      { variant: "warning", title: "Kagan", message: "Add a substantive answer to override this assumption" },
    ])
    expect(clearCalls).toBe(0)
    expect(updateCalls).toBe(0)
    expect(latest).toBe(promptRender)
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
      checkCommand: "bun run check",
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

  test("kagan.retry warns when nothing has failed", async () => {
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
    expect(notices).toContainEqual({ variant: "warning", title: "Kagan", message: "Nothing to retry" })
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
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Retrying intake" })
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
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Retrying review" })
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
    expect(notices).toContainEqual({ variant: "success", title: "Kagan", message: "Retrying review" })
  })

  test("kagan.retry warns for a validator that already ran and is awaiting triage", async () => {
    const session = {
      id: "s1",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorSessionID: "v1", validatorOutcome: "ran" } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.retry")
      ?.run()
    expect(notices).toContainEqual({ variant: "warning", title: "Kagan", message: "Nothing to retry" })
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

  test("kagan.export_packet warns when no task is selected", async () => {
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => undefined,
      sessions: () => [],
      notify: (options: unknown) => notices.push(options),
    })
    await createBoardCommands({} as TuiPluginApi, store, () => {})
      .find((command) => command.name === "kagan.export_packet")
      ?.run()
    expect(notices).toEqual([{ variant: "warning", title: "Kagan", message: "Select a task to export" }])
  })

  test("kagan.export_packet writes a valid trust packet straight to the default path", async () => {
    const tempDir = await mkdtemp(join(tmpdir(), "kagan-export-"))
    tempDirs.push(tempDir)
    const session = {
      id: "s1",
      title: "Add retry",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, taskNumber: 7, generation: 2, approved: true } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    const api = { state: { path: { worktree: tempDir } } } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.export_packet")
      ?.run()
    const expectedPath = `${tempDir}/kagan-export-7.json`
    await waitFor(() => notices.length > 0)
    expect(notices).toContainEqual({
      variant: "success",
      title: "Kagan",
      message: `Exported trust packet to ${expectedPath}`,
    })
    const written: unknown = JSON.parse(await readFile(expectedPath, "utf8"))
    expect(isTrustPacket(written)).toBe(true)
    expect(written).toMatchObject({ taskNumber: 7, generation: 2, approved: true })
  })

  test("kagan.import_packet falls back to the path prompt when the worktree has no exports", async () => {
    const tempDir = await mkdtemp(join(tmpdir(), "kagan-import-"))
    tempDirs.push(tempDir)
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => undefined,
      sessions: () => [],
      notify: (options: unknown) => notices.push(options),
    })
    let dialogRendered: (() => { onConfirm: (path: string) => Promise<void> }) | undefined
    const api = {
      state: { path: { worktree: tempDir } },
      ui: {
        dialog: {
          replace: (render: () => unknown) => {
            dialogRendered = render as () => { onConfirm: (path: string) => Promise<void> }
          },
          clear: () => {},
        },
        DialogPrompt: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.import_packet")
      ?.run()
    await waitFor(() => dialogRendered !== undefined)
    await dialogRendered!().onConfirm(join(tempDir, "does-not-exist.json"))
    expect(notices).toContainEqual({ variant: "error", title: "Kagan", message: "File not found" })
  })

  test("kagan.import_packet lists matching exports newest-first and opens the selected one", async () => {
    const tempDir = await mkdtemp(join(tmpdir(), "kagan-import-"))
    tempDirs.push(tempDir)
    const older = serializeTrustPacket({ kagan: { taskNumber: 3, generation: 1, approved: false } }, [])
    const newer = serializeTrustPacket({ kagan: { taskNumber: 9, generation: 1, approved: false } }, [])
    await writeFile(join(tempDir, "kagan-export-3.json"), JSON.stringify(older))
    await writeFile(join(tempDir, "not-an-export.json"), JSON.stringify({ hello: "world" }))
    await new Promise((resolve) => setTimeout(resolve, 5))
    await writeFile(join(tempDir, "kagan-export-9.json"), JSON.stringify(newer))
    const store = mockStore({ selected: () => undefined, sessions: () => [] })
    const renders: Array<() => unknown> = []
    const api = {
      state: { path: { worktree: tempDir } },
      theme: { current: mockTheme },
      ui: {
        dialog: { replace: (render: () => unknown) => renders.push(render), clear: () => {} },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.import_packet")
      ?.run()
    await waitFor(() => renders.length === 1)
    const selectProps = renders[0]!() as {
      options: { title: string; value: string }[]
      onSelect: (option: { value: string }) => void
    }
    expect(selectProps.options.map((option) => option.title)).toEqual(["kagan-export-9.json", "kagan-export-3.json"])
    selectProps.onSelect(selectProps.options[0]!)
    await waitFor(() => renders.length === 2)
  })

  test("kagan.import_packet errors when the selected export is not a valid trust packet", async () => {
    const tempDir = await mkdtemp(join(tmpdir(), "kagan-import-"))
    tempDirs.push(tempDir)
    await writeFile(join(tempDir, "kagan-export-1.json"), JSON.stringify({ hello: "world" }))
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => undefined,
      sessions: () => [],
      notify: (options: unknown) => notices.push(options),
    })
    const renders: Array<() => unknown> = []
    const api = {
      state: { path: { worktree: tempDir } },
      ui: {
        dialog: { replace: (render: () => unknown) => renders.push(render), clear: () => {} },
        DialogSelect: (props: unknown) => props,
      },
    } as unknown as TuiPluginApi
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.import_packet")
      ?.run()
    await waitFor(() => renders.length === 1)
    const selectProps = renders[0]!() as { options: { value: string }[]; onSelect: (option: { value: string }) => void }
    selectProps.onSelect(selectProps.options[0]!)
    await waitFor(() => notices.length > 0)
    expect(notices).toContainEqual({ variant: "error", title: "Kagan", message: "Invalid trust packet" })
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
      "view",
      "open",
      "advance",
      "send_back",
      "approve",
      "retry",
      "export",
      "import",
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
      ui: {
        dialog: { replace: (render: () => JSX.Element) => renders.push(render), clear: () => {} },
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
    expect(renderSetup.captureCharFrame()).toContain("#4 Add retry")
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
  ): Promise<{ options: { title: string; value: string }[]; onSelect: (option: { value: string }) => void }> {
    await createBoardCommands(api, store, () => {})
      .find((command) => command.name === "kagan.approve")
      ?.run()
    renderSetup = await testRender(renders.at(-1) as () => JSX.Element, { width: 100, height: 24 })
    attachRendererMockInput(api, renderSetup)
    await renderSetup.flush()
    renderSetup.mockInput.pressKey("a")
    await waitFor(() => renders.length === 2)
    return renders.at(-1)!() as {
      options: { title: string; value: string }[]
      onSelect: (option: { value: string }) => void
    }
  }

  test("approving reaches promptAnotherBranch, which warns when the task's own branch is the only local branch", async () => {
    const run = bunGitRunner()
    const repoDir = await mkdtemp(join(tmpdir(), "kagan-cmd-repo-"))
    tempDirs.push(repoDir)
    await run(["init", "-q", "-b", "main"], repoDir)
    await run(["config", "user.email", "test@kagan.dev"], repoDir)
    await run(["config", "user.name", "Kagan Test"], repoDir)
    await writeFile(join(repoDir, "file.txt"), "hello\n")
    await run(["add", "-A"], repoDir)
    await run(["commit", "-q", "-m", "initial"], repoDir)

    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran", worktree: repoDir, baseBranch: "main" } },
    }
    const notices: unknown[] = []
    const store = mockStore({
      selected: () => "s1",
      sessions: () => [session],
      notify: (options: unknown) => notices.push(options),
    })
    const renders: Array<() => unknown> = []
    const api = mockTuiApi({
      state: { path: { worktree: repoDir }, vcs: { branch: "main" } },
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
    mergeProps.onSelect({ value: "another" })
    await waitFor(() => notices.length > 0)
    expect(notices).toContainEqual({
      variant: "warning",
      title: "Kagan",
      message: "No other local branches to merge into",
    })
  })

  test("approving and merging into the current branch surfaces the merge failure without approving", async () => {
    const session = {
      id: "s1",
      title: "Task",
      kaganStatus: "review" as const,
      metadata: { kagan: { boardTask: true, validatorOutcome: "ran" } },
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
    mergeProps.onSelect({ value: "current" })
    await waitFor(() => notices.length > 0)
    expect(notices).toEqual([{ variant: "error", title: "Kagan", message: "Task has no isolated worktree" }])
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
    mergeProps.onSelect({ value: "none" })
    await waitFor(() => sequence.includes("notify:Task approved"))
    expect(sequence).toEqual(["approve", "refresh", "moveTo:done", "notify:Task approved"])
  })
})

describe("openMergeDialog", () => {
  type MergeDialogProps = {
    title: string
    options: { title: string; value: string }[]
    onSelect: (option: { value: string }) => void
  }

  function mockApi(branch = "main") {
    const rendered = { current: undefined as (() => MergeDialogProps) | undefined }
    return {
      api: {
        state: { vcs: { branch }, path: { worktree: "/repo" } },
        ui: {
          dialog: {
            replace: (render: () => MergeDialogProps) => {
              rendered.current = render
            },
            clear: () => {},
          },
          DialogSelect: (props: unknown) => props,
        },
      } as unknown as TuiPluginApi,
      rendered,
    }
  }

  function mockSession(baseBranch?: string): BoardSession {
    return { id: "s1", metadata: { kagan: baseBranch !== undefined ? { baseBranch } : {} } } as unknown as BoardSession
  }

  function mockStore(squash = false): BoardStore {
    return { squashMerge: squash } as unknown as BoardStore
  }

  const handlers: MergeDialogHandlers = {
    runMerge: async () => {},
    promptAnotherBranch: async () => {},
    finalizeApprove: async () => {},
  }

  test("renders the dialog with the standard title when the base is even", () => {
    const { api, rendered } = mockApi()
    openMergeDialog(api, mockStore(), mockSession("main"), { ahead: 0 }, handlers)
    expect(rendered.current).toBeDefined()
    const props = rendered.current!()
    expect(props).toMatchObject({ title: "Approve — merge the task branch?" })
    expect(props.options).toEqual(mergeChoiceOptions("main", false))
  })

  test("includes the freshness notice in the title when the base is ahead", () => {
    const { api, rendered } = mockApi()
    openMergeDialog(api, mockStore(), mockSession("main"), { ahead: 3 }, handlers)
    const props = rendered.current!()
    expect(props.title).toContain("main is 3 commit(s) ahead")
    expect(props.title).toContain("the reviewed diff may be stale")
    expect(props.options).toEqual(mergeChoiceOptions("main", false))
  })

  test("does not include a notice when no base branch is recorded", () => {
    const { api, rendered } = mockApi()
    openMergeDialog(api, mockStore(), mockSession(undefined), { ahead: 3 }, handlers)
    const props = rendered.current!()
    expect(props.title).toBe("Approve — merge the task branch?")
  })

  test("preserves squash-merge labeling when squash is enabled", () => {
    const { api, rendered } = mockApi()
    openMergeDialog(api, mockStore(true), mockSession("main"), { ahead: 2 }, handlers)
    const props = rendered.current!()
    expect(props.title).toContain("main is 2 commit(s) ahead")
    expect(props.options).toEqual(mergeChoiceOptions("main", true))
  })

  test("invokes runMerge when the current branch is selected", () => {
    let mergedBranch: string | undefined
    const { api, rendered } = mockApi()
    openMergeDialog(
      api,
      mockStore(),
      mockSession("main"),
      { ahead: 0 },
      {
        ...handlers,
        runMerge: async (_, branch) => {
          mergedBranch = branch
        },
      },
    )
    const props = rendered.current!()
    props.onSelect({ value: "current" })
    expect(mergedBranch).toBe("main")
  })

  test("invokes promptAnotherBranch when another branch is selected", () => {
    let prompted = false
    const { api, rendered } = mockApi()
    openMergeDialog(
      api,
      mockStore(),
      mockSession("main"),
      { ahead: 0 },
      {
        ...handlers,
        promptAnotherBranch: async () => {
          prompted = true
        },
      },
    )
    const props = rendered.current!()
    props.onSelect({ value: "another" })
    expect(prompted).toBe(true)
  })

  test("invokes finalizeApprove when no action is selected", () => {
    let approved = false
    const { api, rendered } = mockApi()
    openMergeDialog(
      api,
      mockStore(),
      mockSession("main"),
      { ahead: 0 },
      {
        ...handlers,
        finalizeApprove: async () => {
          approved = true
        },
      },
    )
    const props = rendered.current!()
    props.onSelect({ value: "none" })
    expect(approved).toBe(true)
  })
})
