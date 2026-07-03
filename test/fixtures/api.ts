import type { PluginInput } from "@opencode-ai/plugin"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { COLUMNS, ROUTE, type BoardSession, type ColumnType } from "../../src/types"

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
}

export function mockKv(overrides: Record<string, unknown> = {}): Record<string, unknown> {
  const kv: Record<string, unknown> = { ...overrides }
  for (const column of COLUMNS) {
    kv[`kagan:order:${column}`] ??= []
  }
  return kv
}

export function mockTuiApi(overrides: Partial<TuiPluginApi> & { kvMap?: Record<string, unknown> } = {}): TuiPluginApi {
  const kvMap = mockKv(overrides.kvMap)
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
    },
    route: { current: { name: ROUTE } },
    theme: { current: mockTheme },
    keymap: { registerLayer: () => {} },
    ...overrides,
  } as unknown as TuiPluginApi
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
