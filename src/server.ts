import type { Plugin, PluginModule } from "@opencode-ai/plugin"
import type { Permission } from "@opencode-ai/sdk"
import { isSupervisedSession, ownsReadOnlyHelper, readOnlyHelperClaim } from "./domain/task/policy"
import { isGitPushCommand } from "./git/runner"
import { getSessionData } from "./server/data"
import { createServerEvents } from "./server/events"
import { createServerTools } from "./server/tools"
import { buildKaganTaskTemplate } from "./server/command"

const PUSH_DENIED_MESSAGE =
  "kagan task sandboxes cannot push to a remote — merging happens through the board's merge dialog after review."

async function guardGitPush(
  input: Parameters<Plugin>[0],
  hookInput: { tool: string; sessionID: string; callID: string },
  output: { args: { command?: unknown } },
): Promise<void> {
  if (hookInput.tool !== "bash") return
  const command = output.args?.command
  if (typeof command !== "string" || !isGitPushCommand(command)) return
  if (isSupervisedSession((await getSessionData(input, hookInput.sessionID))?.metadata)) {
    throw new Error(PUSH_DENIED_MESSAGE)
  }
}

async function allowReadOnlyHelper(
  input: Parameters<Plugin>[0],
  permission: Permission,
  output: { status: "ask" | "deny" | "allow" },
): Promise<void> {
  const claim = readOnlyHelperClaim((await getSessionData(input, permission.sessionID))?.metadata)
  if (!claim) return
  const parent = await getSessionData(input, claim.parent)
  if (ownsReadOnlyHelper(parent?.metadata, claim.role, permission.sessionID)) output.status = "allow"
}

const server: Plugin = async (input, options) => ({
  "tool.execute.before": (hookInput, output) => guardGitPush(input, hookInput, output),
  "permission.ask": (permission, output) => allowReadOnlyHelper(input, permission, output),
  event: createServerEvents(input, options),
  tool: createServerTools(input, options),
  config: async (cfg) => {
    cfg.command ??= {}
    cfg.command["kagan-task"] = {
      description: "Create Kagan board tasks through a conversational ticket workflow",
      template: buildKaganTaskTemplate(options),
    }
  },
})

const plugin: PluginModule = {
  id: "kagan",
  server,
}

export default plugin
