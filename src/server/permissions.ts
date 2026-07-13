import type { PluginInput } from "@opencode-ai/plugin"
import { kagan } from "../domain/task/metadata"
import { getSessionData } from "./data"
import { resolveOwningBoardTask } from "./helpers/events"
import { mutateKagan } from "./session/patch"

export async function handlePermissionRequested(
  input: PluginInput,
  permission: { id: string; title: string; sessionID: string },
): Promise<void> {
  const role = kagan((await getSessionData(input, permission.sessionID))?.metadata).role
  // Read-only helpers auto-approve their own permissions (server.ts) and cannot spawn subagents,
  // so their asks never block — keep them out of the human queue.
  if (role === "intake" || role === "validator") return
  const rootID = await resolveOwningBoardTask(input, permission.sessionID)
  if (!rootID) return
  await mutateKagan(input.client, rootID, (view) => {
    const current = view.awaitingPermissions ?? []
    if (current.some((entry) => entry.id === permission.id)) return undefined
    return {
      awaitingPermissions: [
        ...current,
        { id: permission.id, title: permission.title, sessionID: permission.sessionID },
      ],
    }
  })
}

export async function handlePermissionReplied(
  input: PluginInput,
  sessionID: string,
  permissionID: string,
): Promise<void> {
  const rootID = await resolveOwningBoardTask(input, sessionID)
  if (!rootID) return
  await mutateKagan(input.client, rootID, (view) => {
    const current = view.awaitingPermissions ?? []
    const next = current.filter((entry) => entry.id !== permissionID)
    if (next.length === current.length) return undefined
    return { awaitingPermissions: next.length > 0 ? next : undefined }
  })
}
