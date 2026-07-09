import { AsyncLocalStorage } from "node:async_hooks"
import type { PluginInput } from "@opencode-ai/plugin"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Session } from "@opencode-ai/sdk/v2"
import { runCommandPlan, truncateCheckResultForMetadata, type CommandSpec } from "./check"
import {
  approveDenyReason,
  commandInTaskScope,
  helper,
  kagan,
  nextGenerationPatch,
  rawKagan,
  resolveFinding,
  resolveIntakeDecision,
  type FindingResolution,
  type HelperRole,
  type ModelRef,
  type TaskScope,
} from "./task"
import {
  bunGitRunner,
  createTaskWorktree,
  currentBranch,
  ensureWorktreePluginConfig,
  mergeTaskBranch,
  uniqueTaskSlug,
  worktreeDiffs,
  type MergeResult,
} from "./git"
import { composeHandoffPrompt } from "./handoff"
import { type BoardSession, type ColumnType } from "./types"

export function getStatus(metadata?: Record<string, unknown>): ColumnType {
  return kagan(metadata).status ?? "backlog"
}

function mergeKagan(current: Record<string, unknown>, partial: Record<string, unknown>): Record<string, unknown> {
  return { ...current, kagan: { ...rawKagan(current), ...partial } }
}

// OpenCode dispatches the plugin `event` hook fire-and-forget, so handlers across the server and
// TUI plugins can run concurrently against the same session. Without serialization, two concurrent
// patches read the same metadata snapshot and the second write clobbers the first (lost update).
// This queues patches per session so each read-modify-write completes before the next one starts.
const sessionLocks = ((globalThis as Record<string, unknown>).__kaganSessionLocks ??= new Map<
  string,
  Promise<unknown>
>()) as Map<string, Promise<unknown>>

// OpenCode also delivers a session.updated event — which can trigger another patch on the same
// session — before the session.update() call that caused it has resolved. That makes the follow-up
// patch a *nested* call arriving on the same logical chain as the lock it would otherwise wait on,
// which would deadlock (the outer call can't finish until the inner one does, and the inner one is
// queued behind the outer). This tracks which sessionIDs are already locked by an ancestor on the
// current continuation so a nested call can run inline instead of queuing behind itself.
const heldSessionLocks = new AsyncLocalStorage<ReadonlySet<string>>()

function withSessionLock<T>(sessionID: string, fn: () => Promise<T>): Promise<T> {
  const held = heldSessionLocks.getStore()
  if (held?.has(sessionID)) return fn()

  const previous = sessionLocks.get(sessionID) ?? Promise.resolve()
  const nextHeld = new Set(held)
  nextHeld.add(sessionID)
  const result = previous.then(() => heldSessionLocks.run(nextHeld, fn))
  // The stored tail always resolves, even when `fn` throws, so a failed patch doesn't wedge the
  // queue for the next caller. The caller's own `result` still rejects on failure.
  const tail: Promise<unknown> = result.then(
    () => undefined,
    () => undefined,
  )
  sessionLocks.set(sessionID, tail)
  void tail.then(() => {
    if (sessionLocks.get(sessionID) === tail) sessionLocks.delete(sessionID)
  })
  return result
}

async function patchCore(
  sessionID: string,
  partial: Record<string, unknown>,
  get: () => Promise<Record<string, unknown>>,
  update: (merged: Record<string, unknown>) => Promise<void>,
): Promise<void> {
  return withSessionLock(sessionID, async () => {
    // Fresh read inside the lock is what makes the read-modify-write atomic per session.
    const currentMetadata = await get()
    const merged = mergeKagan(currentMetadata, partial)
    await update(merged)
  })
}

async function patchKaganWhen(
  client: PluginInput["client"],
  sessionID: string,
  compute: (metadata: Record<string, unknown>) => Record<string, unknown> | undefined,
): Promise<boolean> {
  let applied = false
  await withSessionLock(sessionID, async () => {
    const result = await client.session.get({ path: { id: sessionID }, throwOnError: true })
    const currentMetadata = ((result.data as { metadata?: Record<string, unknown> } | undefined)?.metadata ??
      {}) as Record<string, unknown>
    const partial = compute(currentMetadata)
    if (partial === undefined) return
    applied = true
    const merged = mergeKagan(currentMetadata, partial)
    await client.session.update({
      path: { id: sessionID },
      body: { metadata: merged },
      throwOnError: true,
    } as Parameters<typeof client.session.update>[0])
  })
  return applied
}

export async function claimHelperSpawn(
  client: PluginInput["client"],
  sessionID: string,
  role: HelperRole,
): Promise<boolean> {
  const outcomeField = `${role}Outcome`
  return patchKaganWhen(client, sessionID, (metadata) => {
    const before = helper(metadata, role)
    if (before.outcome !== undefined || before.sessionID !== undefined) return undefined
    return { [outcomeField]: "pending" }
  })
}

export async function patchKagan(
  client: PluginInput["client"],
  sessionID: string,
  partial: Record<string, unknown>,
): Promise<void> {
  return patchCore(
    sessionID,
    partial,
    async () => {
      const result = await client.session.get({ path: { id: sessionID }, throwOnError: true })
      return ((result.data as { metadata?: Record<string, unknown> } | undefined)?.metadata ?? {}) as Record<
        string,
        unknown
      >
    },
    async (merged) => {
      await client.session.update({
        path: { id: sessionID },
        body: { metadata: merged },
        throwOnError: true,
      } as Parameters<typeof client.session.update>[0])
    },
  )
}

// Always does a fresh GET inside the lock. Callers used to be able to pass in a metadata snapshot
// they already held to skip the GET, but under concurrent patches that snapshot can be stale,
// which reintroduces the lost-update race the lock above exists to prevent.
async function tuiPatchKagan(api: TuiPluginApi, sessionID: string, partial: Record<string, unknown>): Promise<void> {
  return patchCore(
    sessionID,
    partial,
    async () => {
      const result = await api.client.session.get({ sessionID }, { throwOnError: true })
      const data = result.data as { metadata?: Record<string, unknown> } | undefined
      return data?.metadata ?? {}
    },
    async (merged) => {
      await api.client.session.update({ sessionID, metadata: merged }, { throwOnError: true })
    },
  )
}

export async function listSessions(api: TuiPluginApi): Promise<Session[]> {
  const result = await api.client.session.list({ scope: "project" }, { throwOnError: true })
  return result.data ?? []
}

export async function moveSession(api: TuiPluginApi, sessionID: string, status: ColumnType): Promise<void> {
  await tuiPatchKagan(api, sessionID, { status })
}

export async function resolveSessionFinding(
  api: TuiPluginApi,
  sessionID: string,
  session: Session,
  findingID: string,
  resolution: FindingResolution,
  note?: string,
): Promise<void> {
  const findings = kagan(session.metadata).findings ?? []
  const updated = resolveFinding(findings, findingID, resolution, note)
  await tuiPatchKagan(api, sessionID, { findings: updated })
}

export async function resolveSessionIntakeDecision(
  api: TuiPluginApi,
  sessionID: string,
  session: Session,
  decisionID: string,
  resolution: "approved" | "overridden",
  answer?: string,
): Promise<void> {
  const intake = kagan(session.metadata).intake
  if (!intake) return
  const decisions = resolveIntakeDecision(intake.decisions, decisionID, resolution, answer)
  await tuiPatchKagan(api, sessionID, { intake: { ...intake, decisions } })
}

export async function retryHelper(
  api: TuiPluginApi,
  sessionID: string,
  session: Session,
  status: ColumnType,
): Promise<void> {
  if (status !== "backlog" && status !== "review") throw new Error("Retry only applies to backlog or review tasks")
  const role: HelperRole = status === "backlog" ? "intake" : "validator"
  await tuiPatchKagan(api, sessionID, {
    [`${role}SessionID`]: undefined,
    [`${role}Outcome`]: undefined,
    [`${role}Attempts`]: 0,
    helperError: undefined,
  })
}

export async function approveSession(api: TuiPluginApi, sessionID: string, session: Session): Promise<void> {
  const reason = approveDenyReason(session.metadata)
  if (reason) throw new Error(reason)
  await tuiPatchKagan(api, sessionID, { approved: true })
}

export async function archiveSession(api: TuiPluginApi, sessionID: string): Promise<void> {
  await api.client.session.update({ sessionID, time: { archived: Date.now() } }, { throwOnError: true })
}

export async function createTask(
  api: TuiPluginApi,
  input: {
    title: string
    description: string
    model?: ModelRef
    baseBranch: string
    setupCommands?: CommandSpec[]
    scope?: TaskScope
  },
): Promise<Session> {
  const existing = await listSessions(api)
  const taskNumber = existing.reduce((max, session) => Math.max(max, kagan(session.metadata).taskNumber ?? 0), 0) + 1
  const slug = uniqueTaskSlug(input.title)
  const { directory } = await createTaskWorktree(bunGitRunner(), api.state.path.worktree, slug, input.baseBranch)
  await ensureWorktreePluginConfig(directory)
  const description = input.description.trim()
  const patch: Record<string, unknown> = {
    status: "backlog",
    boardTask: true,
    taskNumber,
    baseBranch: input.baseBranch,
    worktree: directory,
  }
  if (description) patch.description = description
  if (input.model) patch.model = input.model
  if (input.scope) patch.scope = input.scope
  const setup = await runCommandPlan(
    input.setupCommands ?? [],
    directory,
    (command) => commandInTaskScope(command, input.scope),
    "task scope does not include this cwd",
    false,
  )
  if (setup) patch.setup = truncateCheckResultForMetadata(setup)
  const result = await api.client.session.create(
    {
      directory,
      title: input.title,
      ...(input.model ? { model: { id: input.model.modelID, providerID: input.model.providerID } } : {}),
      metadata: { kagan: patch },
    },
    { throwOnError: true },
  )
  return result.data
}

type MessageEnvelope = { info?: { role?: string }; parts?: Array<{ type?: string; text?: string }> }

export function lastAssistantText(messages: unknown): string | undefined {
  if (!Array.isArray(messages)) return undefined
  for (let index = messages.length - 1; index >= 0; index--) {
    const envelope = messages[index] as MessageEnvelope
    if (envelope?.info?.role !== "assistant") continue
    const text = (envelope.parts ?? [])
      .filter((part) => part.type === "text" && typeof part.text === "string")
      .map((part) => part.text)
      .join("\n")
      .trim()
    if (text) return text
  }
  return undefined
}

export async function sendBack(api: TuiPluginApi, session: BoardSession): Promise<void> {
  const metadata = session.metadata as Record<string, unknown> | undefined
  const view = kagan(metadata)
  const worktree = view.worktree
  if (!worktree) throw new Error("Task has no isolated worktree")
  const baseBranch = view.baseBranch ?? "HEAD"
  const model = view.model
  const previousID = view.activeIteration ?? session.id

  const messages = await api.client.session.messages({ sessionID: previousID }, { throwOnError: true })
  const previousReport = lastAssistantText(messages.data ?? [])
  const changedFiles = (await worktreeDiffs(bunGitRunner(), worktree, baseBranch))
    .map((diff) => diff.file)
    .filter((file): file is string => typeof file === "string")

  const worker = await api.client.session.create(
    {
      directory: worktree,
      parentID: session.id,
      title: `iteration ${view.generation + 1}`,
      metadata: { kagan: { role: "worker", workerParent: session.id } },
    },
    { throwOnError: true },
  )
  await api.client.session.promptAsync(
    {
      sessionID: worker.data.id,
      ...(model ? { model } : {}),
      parts: [
        {
          type: "text",
          text: composeHandoffPrompt({ title: session.title, metadata, previousReport, changedFiles }),
        },
      ],
    },
    { throwOnError: true },
  )
  await tuiPatchKagan(api, session.id, {
    ...nextGenerationPatch(metadata),
    status: "in_progress",
    activeIteration: worker.data.id,
  })
}

export async function mergeTask(
  api: TuiPluginApi,
  session: BoardSession,
  targetBranch: string,
  squash: boolean,
): Promise<MergeResult> {
  const metadata = session.metadata as Record<string, unknown> | undefined
  const worktree = kagan(metadata).worktree
  if (!worktree) return { ok: false, message: "Task has no isolated worktree" }
  const runner = bunGitRunner()
  const branch = await currentBranch(runner, worktree)
  if (!branch) return { ok: false, message: "Cannot determine the task branch" }
  return mergeTaskBranch(
    runner,
    api.state.path.worktree,
    worktree,
    branch,
    targetBranch,
    `kagan: ${session.title}`,
    squash,
  )
}

export async function deleteSession(api: TuiPluginApi, sessionID: string): Promise<void> {
  await api.client.session.delete({ sessionID }, { throwOnError: true })
}

export const orderKey = (column: ColumnType) => `kagan:order:${column}`

export function getOrder(api: TuiPluginApi, column: ColumnType): string[] {
  return api.kv.get(orderKey(column), [])
}

export function setOrder(api: TuiPluginApi, column: ColumnType, order: readonly string[]): void {
  api.kv.set(orderKey(column), [...order])
}

export const filterKey = "kagan:filter"

export function getFilter(api: TuiPluginApi): string {
  return api.kv.get(filterKey, "")
}

export function setFilter(api: TuiPluginApi, value: string): void {
  api.kv.set(filterKey, value)
}
