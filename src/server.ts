import type { Plugin, PluginModule } from "@opencode-ai/plugin"
import { createServerEvents } from "./server/events"
import { guardGitPush } from "./server/push-guard"
import { createServerTools } from "./server/tools"

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
