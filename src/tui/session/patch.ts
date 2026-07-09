import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { lockSessionMetadata, mergeKagan } from "../../domain/session/metadata"

export async function tuiPatchKagan(
  api: TuiPluginApi,
  sessionID: string,
  partial: Record<string, unknown>,
): Promise<void> {
  await lockSessionMetadata(sessionID, async () => {
    const result = await api.client.session.get({ sessionID }, { throwOnError: true })
    const metadata = (result.data as { metadata?: Record<string, unknown> } | undefined)?.metadata ?? {}
    await api.client.session.update({ sessionID, metadata: mergeKagan(metadata, partial) }, { throwOnError: true })
  })
}
