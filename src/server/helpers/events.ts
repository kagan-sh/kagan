import type { PluginInput } from "@opencode-ai/plugin"
import { helper, helperRetries } from "../../domain/task/policy"
import { kagan } from "../../domain/task/metadata"
import type { HelperRole } from "../../domain/task/types"
import { getSessionData } from "../data"
import { patchKagan } from "../session/patch"

export async function handleHelperFailure(
  input: PluginInput,
  role: HelperRole,
  parentSessionID: string,
  attemptsUsed: number,
  message: string,
  options?: Record<string, unknown>,
): Promise<void> {
  const retries = helperRetries(options)
  if (attemptsUsed <= retries) {
    await patchKagan(input.client, parentSessionID, {
      [`${role}SessionID`]: undefined,
      [`${role}Outcome`]: undefined,
      [`${role}Attempts`]: attemptsUsed,
    })
    return
  }
  await patchKagan(input.client, parentSessionID, {
    [`${role}Outcome`]: "failed",
    helperError: { role, message },
  })
}

export async function handleHelperEvent(
  input: PluginInput,
  role: HelperRole,
  sessionID: string,
  metadata: Record<string, unknown> | undefined,
  message: string,
  options?: Record<string, unknown>,
): Promise<void> {
  const parentID = helper(metadata, role).parent
  if (!parentID) return
  const parentHelper = helper((await getSessionData(input, parentID))?.metadata, role)
  if (parentHelper.sessionID !== sessionID || parentHelper.outcome !== "pending") return
  await handleHelperFailure(input, role, parentID, parentHelper.attempts, message, options)
}

export function owningRootTaskID(
  metadata: Record<string, unknown> | undefined,
  sessionID: string,
  parentID?: string | null,
): string | undefined {
  const view = kagan(metadata)
  if (view.role === "intake" || view.role === "validator") return helper(metadata, view.role).parent
  if (view.role === "worker") return view.workerParent
  return view.boardTask === true && !parentID ? sessionID : undefined
}

export async function resolveOwningBoardTask(input: PluginInput, sessionID: string): Promise<string | undefined> {
  const session = await getSessionData(input, sessionID)
  return owningRootTaskID(session?.metadata, sessionID, session?.parentID)
}
