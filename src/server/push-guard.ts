import type { PluginInput } from "@opencode-ai/plugin"
import { isGitPushCommand } from "../git/runner"
import { isSupervisedSession } from "../domain/task/policy"
import { getSessionData } from "./data"

const PUSH_DENIED_MESSAGE =
  "kagan task sandboxes cannot push to a remote — merging happens through the board's merge dialog after review."

export async function guardGitPush(
  input: PluginInput,
  hookInput: { tool: string; sessionID: string; callID: string },
  output: { args: any },
): Promise<void> {
  if (hookInput.tool !== "bash") return
  const command = output.args?.command
  if (typeof command !== "string" || !isGitPushCommand(command)) return
  if (isSupervisedSession((await getSessionData(input, hookInput.sessionID))?.metadata))
    throw new Error(PUSH_DENIED_MESSAGE)
}
