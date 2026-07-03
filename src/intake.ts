import type { PluginInput } from "@opencode-ai/plugin"
import { patchKagan } from "./session-api"

export async function spawnIntake(
  input: PluginInput,
  parentSessionID: string,
  task: { title: string; description?: string; references?: string },
  options?: Record<string, unknown>,
): Promise<string | undefined> {
  const child = await input.client.session.create({
    body: {
      parentID: parentSessionID,
      title: "task prep",
      metadata: {
        kagan: {
          intakeParent: parentSessionID,
          role: "intake",
        },
      },
    },
    throwOnError: true,
  } as Parameters<typeof input.client.session.create>[0])

  const childID = child.data?.id
  if (!childID) return undefined

  await patchKagan(input.client, parentSessionID, {
    intakeSessionID: childID,
    intakeOutcome: "pending",
  })

  const promptText = [
    "A human is about to start this task:",
    `"${task.title}"`,
    ...(task.description ? ["", task.description] : []),
    "",
    "Read the repository (this checkout is the branch the task will start from) to understand what this task",
    "implies before any code is written. Then call the kagan_intake tool with:",
    "- understanding: a short summary of what this task means and how you would approach it.",
    "- decisions: assumptions that test both your comprehension and the human's — each with the question,",
    "  your default assumption, and required:false only when the work can proceed safely either way.",
    "- refinedPrompt: the final, self-contained instruction the implementing agent will receive,",
    "  incorporating your codebase analysis.",
    "- mode: assess this task against five factors — is there a cheap automatic check that catches a wrong",
    "  result; how costly or irreversible is a bad merge; does the human need to understand every line; is it",
    "  specifiable in one sitting; is it common or novel — and recommend a mode (autonomous / assisted / manual)",
    "  with a one-line rationale. This is advice for the human, not a gate.",
    "If nothing needs confirming, pass an empty decisions array.",
    "Assess whether this task fits a single focused implementation session. If it does not, include",
    "a required decision proposing how to split it into smaller tasks.",
    ...(task.references ? ["", task.references] : []),
  ].join("\n")

  const body: Record<string, unknown> = {
    tools: { read: true, edit: false, write: false, bash: false, kagan_intake: true },
    parts: [{ type: "text", text: promptText }],
  }
  if (typeof options?.intakeAgent === "string") {
    body.agent = options.intakeAgent
  }

  await input.client.session.promptAsync({
    path: { id: childID },
    body,
    throwOnError: true,
  } as Parameters<typeof input.client.session.promptAsync>[0])

  return childID
}
