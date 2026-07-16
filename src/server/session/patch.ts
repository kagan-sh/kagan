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

export async function patchKagan(
  client: PluginInput["client"],
  sessionID: string,
  partial: Record<string, unknown>,
): Promise<void> {
  await lockSessionMetadata(sessionID, async () => {
    const metadata = await readSessionMetadata(client, sessionID)
    await client.session.update({
      path: { id: sessionID },
      body: { metadata: mergeKagan(metadata, partial) },
      throwOnError: true,
    } as Parameters<typeof client.session.update>[0])
  })
}

export async function mutateKagan(
  client: PluginInput["client"],
  sessionID: string,
  compute: (view: ReturnType<typeof kagan>) => Record<string, unknown> | undefined,
): Promise<void> {
  await lockSessionMetadata(sessionID, async () => {
    const metadata = await readSessionMetadata(client, sessionID)
    const patch = compute(kagan(metadata))
    if (!patch) return
    await client.session.update({
      path: { id: sessionID },
      body: { metadata: mergeKagan(metadata, patch) },
      throwOnError: true,
    } as Parameters<typeof client.session.update>[0])
  })
}

export async function claimHelperSpawn(
  client: PluginInput["client"],
  sessionID: string,
  role: HelperRole,
): Promise<boolean> {
  let claimed = false
  await lockSessionMetadata(sessionID, async () => {
    const metadata = await readSessionMetadata(client, sessionID)
    const before = helper(metadata, role)
    if (before.outcome !== undefined || before.sessionID !== undefined) return
    claimed = true
    await client.session.update({
      path: { id: sessionID },
      body: { metadata: mergeKagan(metadata, { [`${role}Outcome`]: "pending" }) },
      throwOnError: true,
    } as Parameters<typeof client.session.update>[0])
  })
  return claimed
}
