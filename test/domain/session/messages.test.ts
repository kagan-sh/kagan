import { describe, expect, test } from "bun:test"
import { lastAssistantText } from "../../../src/domain/session/messages"

describe("lastAssistantText", () => {
  test("returns undefined for non-array input", () => {
    expect(lastAssistantText(undefined)).toBeUndefined()
    expect(lastAssistantText(null)).toBeUndefined()
    expect(lastAssistantText("not an array")).toBeUndefined()
  })

  test("skips assistant messages whose text parts are empty or blank", () => {
    const messages = [
      { info: { role: "assistant" }, parts: [{ type: "text", text: "   " }] },
      { info: { role: "assistant" }, parts: [{ type: "tool" }] },
      { info: { role: "assistant" }, parts: [{ type: "text", text: "Fixed the retry loop." }] },
    ]
    expect(lastAssistantText(messages)).toBe("Fixed the retry loop.")
  })

  test("picks the last of several assistant messages, ignoring user messages after it", () => {
    const messages = [
      { info: { role: "assistant" }, parts: [{ type: "text", text: "First pass done." }] },
      { info: { role: "user" }, parts: [{ type: "text", text: "Please also handle timeouts." }] },
      { info: { role: "assistant" }, parts: [{ type: "text", text: "Handled timeouts too." }] },
    ]
    expect(lastAssistantText(messages)).toBe("Handled timeouts too.")
  })
})
