import type { Plugin, PluginModule } from "@opencode-ai/plugin"
import type { Permission } from "@opencode-ai/sdk"
import { configuredScopes } from "./domain/task/commands"
import { isSupervisedSession, ownsReadOnlyHelper, readOnlyHelperClaim } from "./domain/task/policy"
import { isGitPushCommand } from "./git/runner"
import { getSessionData } from "./server/data"
import { createServerEvents } from "./server/events"
import { createServerTools } from "./server/tools"

export function buildKaganTaskTemplate(options?: Record<string, unknown>): string {
  const scopes = configuredScopes(options)
  const scopeLines =
    scopes.length > 0
      ? [
          "Configured scope cwd values for this project:",
          ...scopes.map((cwd) => `- \`${cwd}\``),
          "When proposing tickets, pick scope values from this list (or omit scope).",
        ]
      : ["No configured scope cwd values — omit scope on tickets unless the user names one explicitly."]

  return [
    "You are helping the user create one or more Kagan board tasks through conversation.",
    "",
    "Workflow:",
    "1. Decide the source of the tickets. This command runs inline in the user's current session, so its conversation is available to you. IF this session already holds substantial prior work or discussion, ask the user EXACTLY ONCE, in one short question, whether to base the tickets on the conversation so far or only on what they typed after the command, then follow their choice. IF the session is empty or has no relevant prior context, skip that question and plan directly from $ARGUMENTS. When basing tickets on the conversation, distill what was discussed or built into concrete tickets instead of asking the user to repeat it.",
    "2. Read $ARGUMENTS and any follow-up from the user. Ask clarifying questions until each task has a clear title and description.",
    "3. Propose a numbered ticket list (title, description, optional base branch, optional scope).",
    "4. Iterate until the user confirms the list is final.",
    "5. Call `kagan_create_tasks` once with every confirmed ticket.",
    "6. Report the tool result to the user.",
    "",
    ...scopeLines,
    "",
    "Rules:",
    "- Never create tasks without explicit user confirmation.",
    "- Each ticket needs a non-blank title and description.",
    "- At most 10 tickets per call.",
    "- Default base branch to the current git branch when omitted.",
    "",
    "$ARGUMENTS",
  ].join("\n")
}

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
