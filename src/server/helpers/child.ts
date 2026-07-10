import type { PluginInput } from "@opencode-ai/plugin"
import type { HelperRole } from "../../domain/task/types"
import { patchKagan } from "../session/patch"

export async function createHelperChild(
  input: PluginInput,
  parentSessionID: string,
  role: HelperRole,
  title: string,
): Promise<string | undefined> {
  const parentField = role === "intake" ? "intakeParent" : "validatorParent"
  const sessionIDField = role === "intake" ? "intakeSessionID" : "validatorSessionID"
  const child = await input.client.session.create({
    body: {
      parentID: parentSessionID,
      title,
      metadata: {
        kagan: {
          [parentField]: parentSessionID,
          role,
        },
      },
    },
    throwOnError: true,
  } as Parameters<typeof input.client.session.create>[0])
  const childID = child.data?.id
  if (!childID) return undefined
  await patchKagan(input.client, parentSessionID, { [sessionIDField]: childID })
  return childID
}
