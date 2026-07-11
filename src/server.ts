import type { Plugin, PluginModule } from "@opencode-ai/plugin"
import { isSupervisedSession } from "./domain/task/policy"
import { isGitPushCommand } from "./git/runner"
import { getSessionData } from "./server/data"
import { createServerEvents } from "./server/events"
import { createServerTools } from "./server/tools"

const PUSH_DENIED_MESSAGE =
  "kagan task sandboxes cannot push to a remote — merging happens through the board's merge dialog after review."

async function guardGitPush(
  input: Parameters<Plugin>[0],
  hookInput: { tool: string; sessionID: string; callID: string },
  output: { args: any },
): Promise<void> {
  if (hookInput.tool !== "bash") return
  const command = output.args?.command
  if (typeof command !== "string" || !isGitPushCommand(command)) return
  if (isSupervisedSession((await getSessionData(input, hookInput.sessionID))?.metadata)) {
    throw new Error(PUSH_DENIED_MESSAGE)
  }
}

const server: Plugin = async (input, options) => ({
  "tool.execute.before": (hookInput, output) => guardGitPush(input, hookInput, output),
  event: createServerEvents(input, options),
  tool: createServerTools(input),
})

const plugin: PluginModule = {
  id: "kagan",
  server,
}

export default plugin
