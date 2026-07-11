import type { PluginInput } from "@opencode-ai/plugin"
import { tool } from "@opencode-ai/plugin/tool"
import { isSupervisedSession } from "../domain/task/policy"
import { getSessionData } from "./data"
import { runCreateTasks, ticketSummary } from "./create-tasks"

const createTasksArgs = {
  tickets: tool.schema
    .array(
      tool.schema.object({
        title: tool.schema.string().describe("Short task title"),
        description: tool.schema.string().describe("Full task description for intake and implementation"),
        baseBranch: tool.schema.string().optional().describe("Git branch to fork from; defaults to current branch"),
        scope: tool.schema
          .object({
            values: tool.schema.array(tool.schema.string()).optional(),
            custom: tool.schema.string().optional(),
          })
          .optional()
          .describe("Task scope for setup/check commands"),
      }),
    )
    .min(1)
    .max(10)
    .describe("Board tasks to create after user confirmation"),
}

export function createTasksTool(input: PluginInput, options?: Record<string, unknown>) {
  return tool({
    description: "Create one or more Kagan board tasks after the user confirms the proposed ticket list",
    args: createTasksArgs,
    async execute(args, ctx) {
      const caller = await getSessionData(input, ctx.sessionID)
      if (isSupervisedSession(caller?.metadata)) {
        throw new Error("kagan_create_tasks is only available in regular OpenCode sessions")
      }
      const tickets = args.tickets.map((ticket) => ({
        title: ticket.title,
        description: ticket.description,
        baseBranch: ticket.baseBranch,
        scope: ticket.scope,
      }))
      try {
        await ctx.ask({
          permission: "kagan_create_tasks",
          patterns: ["*"],
          always: ["kagan_create_tasks"],
          metadata: { tickets: ticketSummary(tickets) },
        })
      } catch (error) {
        throw new Error(error instanceof Error ? error.message : "Task creation denied")
      }
      const output = await runCreateTasks(input, options, tickets)
      return { output }
    },
  })
}
