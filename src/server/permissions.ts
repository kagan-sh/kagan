import type { PluginInput } from "@opencode-ai/plugin"
import { ownsReadOnlyHelper, readOnlyHelperClaim } from "../domain/task/policy"
import { getSessionData } from "./data"
import { resolveOwningBoardTask } from "./helpers/events"
import { mutateKagan } from "./session/patch"

export async function handlePermissionRequested(
  input: PluginInput,
  permission: { id: string; title: string; sessionID: string },
): Promise<void> {
  const claim = readOnlyHelperClaim((await getSessionData(input, permission.sessionID))?.metadata)
  // Verified read-only helpers auto-approve (server.ts) and never block, so skip the human queue;
  // a forged role the owning board task doesn't confirm falls through and gets queued.
  if (claim) {
    const parent = await getSessionData(input, claim.parent)
    if (ownsReadOnlyHelper(parent?.metadata, claim.role, permission.sessionID)) return
  }
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
