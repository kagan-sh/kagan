import type { PluginInput } from "@opencode-ai/plugin"
import type { KeyEvent, TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { KeyInput, TestRendererSetup } from "@opentui/core/testing"
import type { KeyInputContext } from "@opentui/keymap"
import { ROUTE, type BoardSession } from "../../src/tui/types"
import { COLUMNS, type ColumnType } from "../../src/domain/task/types"

export const mockTheme = {
  text: "white",
  textMuted: "gray",
  border: "blue",
  borderActive: "red",
  primary: "magenta",
  accent: "magenta",
  selectedListItemText: "cyan",
  background: "black",
  backgroundPanel: "black",
  info: "cyan",
  success: "green",
  warning: "yellow",
  error: "red",
  markdownText: "white",
  markdownHeading: "magenta",
  markdownLink: "cyan",
  markdownLinkText: "cyan",
  markdownCode: "cyan",
  markdownBlockQuote: "gray",
  markdownEmph: "white",
  markdownStrong: "white",
  markdownHorizontalRule: "gray",
  markdownListItem: "white",
  markdownListEnumeration: "white",
  markdownImage: "cyan",
  markdownImageText: "cyan",
  markdownCodeBlock: "cyan",
  syntaxComment: "gray",
  syntaxKeyword: "magenta",
  syntaxFunction: "cyan",
  syntaxVariable: "white",
  syntaxString: "green",
  syntaxNumber: "yellow",
  syntaxType: "cyan",
  syntaxOperator: "gray",
  syntaxPunctuation: "gray",
}

function mockKv(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const kv: Record<string, unknown> = { ...overrides }
  for (const column of COLUMNS) {
    kv[`kagan:order:${column}`] ??= []
  }
  return kv
}

export function mockTuiApi(overrides: object = {}): TuiPluginApi {
  const {
    kvMap: kvOverrides,
    ui: uiOverride,
    renderer: rendererOverride,
    keymap: keymapOverride,
    ...rest
  } = overrides as {
    kvMap?: Record<string, unknown>
    ui?: Record<string, unknown>
    renderer?: Record<string, unknown>
    keymap?: Record<string, unknown>
    [key: string]: unknown
  }
  const kvMap = mockKv(kvOverrides)
  const keyHandlers = new Set<(key: KeyEvent) => void>()
  const interceptHandlers = new Set<(ctx: KeyInputContext) => void>()
  const keyInput: {
    on: (event: string, handler: (key: KeyEvent) => void) => void
    off: (event: string, handler: (key: KeyEvent) => void) => void
    emitKey: (key: KeyEvent) => void
    lastConsumed?: boolean
  } = {
    on: (event: string, handler: (key: KeyEvent) => void) => {
      if (event === "keypress") keyHandlers.add(handler)
    },
    off: (event: string, handler: (key: KeyEvent) => void) => {
      if (event === "keypress") keyHandlers.delete(handler)
    },
    emitKey: (key: KeyEvent) => {
      let consumed = false
      const ctx = {
        event: key,
        setData: () => {},
        getData: () => undefined,
        consume: () => {
          consumed = true
        },
      }
      for (const handler of Array.from(interceptHandlers)) handler(ctx)
      for (const handler of Array.from(keyHandlers)) handler(key)
      keyInput.lastConsumed = consumed
    },
  }
  const keymap = {
    registerLayer: () => {},
    intercept: (name: "key", handler: (ctx: KeyInputContext) => void) => {
      interceptHandlers.add(handler)
      return () => interceptHandlers.delete(handler)
    },
    ...(keymapOverride as object | undefined),
  }
  return {
    kv: {
      get: (key: string, defaultValue: unknown) => (key in kvMap ? kvMap[key] : defaultValue),
      set: (key: string, value: unknown) => {
        kvMap[key] = value
      },
    },
    ui: {
      toast: () => {},
      dialog: { open: false },
      ...(uiOverride as object | undefined),
    },
    route: { current: { name: ROUTE } },
    theme: { current: mockTheme },
    keymap,
    renderer: {
      width: 120,
      height: 40,
      on: () => {},
      off: () => {},
      ...(rendererOverride as object | undefined),
      keyInput: (rendererOverride as { keyInput?: typeof keyInput } | undefined)?.keyInput ?? keyInput,
    },
    ...rest,
  } as unknown as TuiPluginApi
}

export function attachRendererMockInput(api: TuiPluginApi, setup: TestRendererSetup) {
  const emit = (api.renderer.keyInput as unknown as { emitKey?: (key: KeyEvent) => void }).emitKey
  if (!emit) return
  const pressKey = setup.mockInput.pressKey.bind(setup.mockInput)
  const pressEnter = setup.mockInput.pressEnter.bind(setup.mockInput)
  const pressEscape = setup.mockInput.pressEscape.bind(setup.mockInput)
  const pressTab = setup.mockInput.pressTab.bind(setup.mockInput)
  const pressArrow = setup.mockInput.pressArrow.bind(setup.mockInput)
  const emitAndRender = (key: KeyEvent) => {
    emit(key)
    setup.renderer.requestRender()
  }
  setup.mockInput.pressKey = (key, modifiers) => {
    pressKey(key, modifiers)
    emitAndRender(mockKeyEvent(key, modifiers))
  }
  setup.mockInput.pressEnter = (modifiers) => {
    pressEnter(modifiers)
    emitAndRender(mockKeyEvent("RETURN", modifiers))
  }
  setup.mockInput.pressEscape = (modifiers) => {
    pressEscape(modifiers)
    emitAndRender(mockKeyEvent("ESCAPE", modifiers))
  }
  setup.mockInput.pressTab = (modifiers) => {
    pressTab(modifiers)
    emitAndRender(mockKeyEvent("TAB", modifiers))
  }
  setup.mockInput.pressArrow = (direction, modifiers) => {
    pressArrow(direction, modifiers)
    emitAndRender(mockKeyEvent(`ARROW_${direction.toUpperCase()}` as KeyInput, modifiers))
  }
}

function mockKeyEvent(
  key: KeyInput,
  modifiers: { shift?: boolean; ctrl?: boolean; meta?: boolean; super?: boolean; hyper?: boolean } = {},
): KeyEvent {
  const name =
    key === "RETURN"
      ? "return"
      : key === "ESCAPE"
        ? "escape"
        : key === "TAB"
          ? "tab"
          : key === "ARROW_UP"
            ? "up"
            : key === "ARROW_DOWN"
              ? "down"
              : key === "ARROW_LEFT"
                ? "left"
                : key === "ARROW_RIGHT"
                  ? "right"
                  : typeof key === "string" && key.length === 1
                    ? key.toLowerCase()
                    : key
  let defaultPrevented = false
  let propagationStopped = false
  return {
    name,
    ctrl: !!modifiers.ctrl,
    shift: !!modifiers.shift,
    meta: !!modifiers.meta,
    super: !!modifiers.super,
    hyper: !!modifiers.hyper,
    get defaultPrevented() {
      return defaultPrevented
    },
    get propagationStopped() {
      return propagationStopped
    },
    preventDefault() {
      defaultPrevented = true
    },
    stopPropagation() {
      propagationStopped = true
    },
  } as KeyEvent
}

export function mockSession(
  id: string,
  status: ColumnType,
  title: string,
  updated = 0,
  parentID?: string,
  overrides: Partial<BoardSession> = {},
): BoardSession {
  return {
    id,
    slug: id,
    title,
    projectID: "project",
    directory: "/tmp",
    version: "1",
    time: { created: 0, updated },
    metadata: { kagan: { status, boardTask: true } },
    kaganStatus: status,
    ...(parentID ? { parentID } : {}),
    ...overrides,
  } satisfies BoardSession
}

/**
 * Capture-style mock for the v2-dialect TuiPluginApi.client.session surface (get/update/create/
 * promptAsync/messages) used by commands.tsx and session-api.ts callers. Pass overrides to
 * customize a method's behavior; unset methods default to a capture into the returned `capture`.
 */
export type SessionClientOverrides = {
  metadata?: Record<string, unknown>
  get?: () => Promise<{ data: { metadata?: Record<string, unknown> } }>
  update?: (parameters: Record<string, unknown>) => unknown
  createResult?: { id: string }
  create?: () => Promise<{ data: { id: string } }>
  messagesResult?: unknown[]
}

export type SessionClientCapture = {
  updateArg?: Record<string, unknown>
  createCalls: number
}

export function mockSessionClient(overrides: SessionClientOverrides = {}): {
  client: { session: Record<string, unknown> }
  capture: SessionClientCapture
} {
  const capture: SessionClientCapture = { createCalls: 0 }
  const session = {
    get: overrides.get ?? (async () => ({ data: { metadata: overrides.metadata } })),
    update:
      overrides.update ??
      ((parameters: Record<string, unknown>) => {
        capture.updateArg = parameters
      }),
    create:
      overrides.create ??
      (async () => {
        capture.createCalls++
        return { data: overrides.createResult ?? { id: "worker1" } }
      }),
    promptAsync: async () => {},
    messages: async () => ({ data: overrides.messagesResult ?? [] }),
  }
  return { client: { session }, capture }
}

/**
 * PluginInput factory for intake/validator helper-spawn tests: a fixed /tmp/worktree plus a
 * client.session surface where create/update/promptAsync capture their raw call arguments and
 * optionally forward to a hook (hooks that throw propagate, e.g. to test promptAsync failures).
 */
export type SpawnInputOverrides = {
  createResult?: { id?: string }
  onCreate?: (options: unknown) => void
  onUpdate?: (options: unknown) => void
  onPrompt?: (options: unknown) => void
}

export type SpawnInputCapture = {
  promptBody?: Record<string, unknown>
  updates: Array<Record<string, unknown>>
}

export function mockSpawnInput(overrides: SpawnInputOverrides = {}): {
  input: PluginInput
  capture: SpawnInputCapture
} {
  const capture: SpawnInputCapture = { updates: [] }
  const input = {
    client: {
      session: {
        create: async (options: unknown) => {
          overrides.onCreate?.(options)
          return { data: overrides.createResult ?? { id: "child-1" } }
        },
        get: async () => ({ data: { metadata: {} } }),
        update: async (options: unknown) => {
          const metadata = (options as { body?: { metadata?: Record<string, unknown> } }).body?.metadata
          capture.updates.push(metadata ?? {})
          overrides.onUpdate?.(options)
          return { data: undefined }
        },
        promptAsync: async (options: unknown) => {
          capture.promptBody = (options as { body?: Record<string, unknown> }).body
          overrides.onPrompt?.(options)
          return { data: undefined }
        },
      },
    },
    worktree: "/tmp/worktree",
  } as unknown as PluginInput
  return { input, capture }
}
