import type { PluginInput } from "@opencode-ai/plugin"
import { lockSessionMetadata, mergeKagan } from "../../domain/session/metadata"
import { kagan } from "../../domain/task/metadata"
import { helper } from "../../domain/task/policy"
import type { HelperRole } from "../../domain/task/types"

async function readSessionMetadata(client: PluginInput["client"], sessionID: string): Promise<Record<string, unknown>> {
  const result = await client.session.get({ path: { id: sessionID }, throwOnError: true })
  return ((result.data as { metadata?: Record<string, unknown> } | undefined)?.metadata ?? {}) as Record<
    string,
    unknown
  >
}

async function withKaganUpdate(
  client: PluginInput["client"],
  sessionID: string,
  compute: (metadata: Record<string, unknown>) => Record<string, unknown> | undefined,
): Promise<boolean> {
  let updated = false
  await lockSessionMetadata(sessionID, async () => {
    const metadata = await readSessionMetadata(client, sessionID)
    const patch = compute(metadata)
    if (!patch) return
    updated = true
    await client.session.update({
      path: { id: sessionID },
      body: { metadata: mergeKagan(metadata, patch) },
      throwOnError: true,
    } as Parameters<typeof client.session.update>[0])
  })
  return updated
}

export async function patchKagan(
  client: PluginInput["client"],
  sessionID: string,
  partial: Record<string, unknown>,
): Promise<void> {
  await withKaganUpdate(client, sessionID, () => partial)
}

export async function mutateKagan(
  client: PluginInput["client"],
  sessionID: string,
  compute: (view: ReturnType<typeof kagan>) => Record<string, unknown> | undefined,
): Promise<void> {
  await withKaganUpdate(client, sessionID, (metadata) => compute(kagan(metadata)))
}

export async function claimHelperSpawn(
  client: PluginInput["client"],
  sessionID: string,
  role: HelperRole,
): Promise<boolean> {
  return withKaganUpdate(client, sessionID, (metadata) => {
    const before = helper(metadata, role)
    if (before.outcome !== undefined || before.sessionID !== undefined) return
    return { [`${role}Outcome`]: "pending" }
  })
}
