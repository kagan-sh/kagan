import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { kagan } from "../../domain/task/metadata"

async function stopSession(api: TuiPluginApi, id: string): Promise<void> {
  try {
    await api.client.session.abort({ sessionID: id }, { throwOnError: true })
  } catch {
    // already idle or gone
  }
}

async function removeSession(api: TuiPluginApi, id: string): Promise<void> {
  try {
    await api.client.session.delete({ sessionID: id }, { throwOnError: true })
  } catch {
    // may already be removed with the parent
  }
}

async function relatedSessionIDs(api: TuiPluginApi, sessionID: string): Promise<Set<string>> {
  const related = new Set<string>()
  try {
    const result = await api.client.session.get({ sessionID }, { throwOnError: true })
    const view = kagan(result.data?.metadata)
    if (view.intakeSessionID) related.add(view.intakeSessionID)
    if (view.validatorSessionID) related.add(view.validatorSessionID)
    if (view.activeIteration && view.activeIteration !== sessionID) related.add(view.activeIteration)
  } catch {
    // proceed with delete anyway
  }
  try {
    const children = await api.client.session.children({ sessionID }, { throwOnError: true })
    for (const child of children.data ?? []) if (child.id) related.add(child.id)
  } catch {
    // children lookup is best-effort
  }
  related.delete(sessionID)
  return related
}

export async function deleteSession(api: TuiPluginApi, sessionID: string): Promise<void> {
  const related = await relatedSessionIDs(api, sessionID)
  await Promise.all([...related, sessionID].map((id) => stopSession(api, id)))
  for (const id of related) await removeSession(api, id)
  await api.client.session.delete({ sessionID }, { throwOnError: true })
}
