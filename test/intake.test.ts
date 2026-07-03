import { describe, expect, test } from "bun:test"
import { spawnIntake } from "../src/intake"
import { mockSpawnInput } from "./fixtures/api"

describe("spawnIntake", () => {
  test("returns undefined and never prompts when session.create yields no child id", async () => {
    const { input, capture } = mockSpawnInput({ createResult: {} })
    const childID = await spawnIntake(input, "parent-1", { title: "Add retry logic" })
    expect(childID).toBeUndefined()
    expect(capture.promptBody).toBeUndefined()
  })

  test("prompts the child read-only with the task, refinedPrompt instruction, and the kagan_intake tool", async () => {
    const { input, capture } = mockSpawnInput()
    const childID = await spawnIntake(input, "parent-1", {
      title: "Migrate billing to usage-based pricing",
      description: "Swap the seat model for metered usage across the API surface.",
    })
    expect(childID).toBe("child-1")
    expect(capture.promptBody?.tools).toEqual({
      read: true,
      edit: false,
      write: false,
      bash: false,
      kagan_intake: true,
    })
    const parts = capture.promptBody?.parts as Array<{ type: string; text?: string }>
    const text = parts.find((part) => part.type === "text")?.text
    expect(text).toContain("Migrate billing to usage-based pricing")
    expect(text).toContain("Swap the seat model for metered usage")
    expect(text).toContain("kagan_intake")
    expect(text).toContain("refinedPrompt")
  })

  test("instructs assessing whether the task fits a single session and proposing a split otherwise", async () => {
    const { input, capture } = mockSpawnInput()
    await spawnIntake(input, "parent-1", { title: "Task" })
    const parts = capture.promptBody?.parts as Array<{ type: string; text?: string }>
    const text = parts.find((part) => part.type === "text")?.text
    expect(text).toContain("split it into smaller tasks")
  })

  test("asks for a mode assessment against the five factors and frames it as advice, not a gate", async () => {
    const { input, capture } = mockSpawnInput()
    await spawnIntake(input, "parent-1", { title: "Task" })
    const parts = capture.promptBody?.parts as Array<{ type: string; text?: string }>
    const text = parts.find((part) => part.type === "text")?.text
    expect(text).toContain("not a gate")
  })

  test("includes the description text only when the task has a description", async () => {
    const withDescription = mockSpawnInput()
    await spawnIntake(withDescription.input, "parent-1", { title: "Task", description: "Handle the retry edge case." })
    const withParts = withDescription.capture.promptBody?.parts as Array<{ type: string; text?: string }>
    const withText = withParts.find((part) => part.type === "text")?.text
    expect(withText).toContain("Handle the retry edge case.")

    const withoutDescription = mockSpawnInput()
    await spawnIntake(withoutDescription.input, "parent-1", { title: "Task" })
    const withoutParts = withoutDescription.capture.promptBody?.parts as Array<{ type: string; text?: string }>
    const withoutText = withoutParts.find((part) => part.type === "text")?.text
    expect(withoutText).not.toContain("Handle the retry edge case.")
  })

  test("omits agent unless intakeAgent is configured in options", async () => {
    const { input, capture } = mockSpawnInput()
    await spawnIntake(input, "parent-1", { title: "Task" })
    expect(capture.promptBody?.agent).toBeUndefined()

    const override = mockSpawnInput()
    await spawnIntake(override.input, "parent-1", { title: "Task" }, { intakeAgent: "plan" })
    expect(override.capture.promptBody?.agent).toBe("plan")
  })

  test("normalizes and rethrows when promptAsync fails", async () => {
    const { input } = mockSpawnInput({
      onPrompt: () => {
        throw new Error("ProviderModelNotFoundError: model not found")
      },
    })
    await expect(spawnIntake(input, "parent-1", { title: "Task" })).rejects.toThrow(
      "ProviderModelNotFoundError: model not found",
    )
  })

  test("patches intakeSessionID and pending outcome before prompting", async () => {
    const order: string[] = []
    const { input } = mockSpawnInput({
      onUpdate: (options) => {
        order.push("update")
        const metadata = (options as { body?: { metadata?: Record<string, unknown> } }).body?.metadata
        expect((metadata?.kagan as Record<string, unknown> | undefined)?.intakeOutcome).toBe("pending")
        expect((metadata?.kagan as Record<string, unknown> | undefined)?.intakeSessionID).toBe("child-1")
      },
      onPrompt: () => order.push("prompt"),
    })
    await spawnIntake(input, "parent-1", { title: "Task" })
    expect(order).toEqual(["update", "prompt"])
  })
})
