import { describe, expect, test } from "bun:test"
import { parseOptions } from "../src/options"

describe("parseOptions defaults", () => {
  test("returns defaults when options are absent, empty, or garbage", () => {
    for (const raw of [undefined, {}, null, "garbage", 42, true]) {
      expect(parseOptions(raw)).toEqual({
        inProgressLimit: 2,
        helperRetries: 1,
        sendBackStopThreshold: 3,
        squashMerge: true,
        intakeAgent: undefined,
        validatorAgent: undefined,
      })
    }
  })
})

describe("parseOptions inProgressLimit", () => {
  test("reads an integer >= 1", () => {
    expect(parseOptions({ inProgressLimit: 5 }).inProgressLimit).toBe(5)
    expect(parseOptions({ inProgressLimit: 1 }).inProgressLimit).toBe(1)
  })

  test("falls back on non-integer, zero, negative, and wrong type", () => {
    expect(parseOptions({ inProgressLimit: 0 }).inProgressLimit).toBe(2)
    expect(parseOptions({ inProgressLimit: -3 }).inProgressLimit).toBe(2)
    expect(parseOptions({ inProgressLimit: 2.5 }).inProgressLimit).toBe(2)
    expect(parseOptions({ inProgressLimit: "3" }).inProgressLimit).toBe(2)
  })
})

describe("parseOptions helperRetries", () => {
  test("reads an integer >= 0", () => {
    expect(parseOptions({ helperRetries: 0 }).helperRetries).toBe(0)
    expect(parseOptions({ helperRetries: 3 }).helperRetries).toBe(3)
  })

  test("falls back on negative, non-integer, and wrong type", () => {
    expect(parseOptions({ helperRetries: -1 }).helperRetries).toBe(1)
    expect(parseOptions({ helperRetries: 1.5 }).helperRetries).toBe(1)
    expect(parseOptions({ helperRetries: "2" }).helperRetries).toBe(1)
  })
})

describe("parseOptions sendBackStopThreshold", () => {
  test("reads an integer >= 1", () => {
    expect(parseOptions({ sendBackStopThreshold: 1 }).sendBackStopThreshold).toBe(1)
    expect(parseOptions({ sendBackStopThreshold: 7 }).sendBackStopThreshold).toBe(7)
  })

  test("falls back on zero, non-integer, and wrong type", () => {
    expect(parseOptions({ sendBackStopThreshold: 0 }).sendBackStopThreshold).toBe(3)
    expect(parseOptions({ sendBackStopThreshold: 2.5 }).sendBackStopThreshold).toBe(3)
    expect(parseOptions({ sendBackStopThreshold: "4" }).sendBackStopThreshold).toBe(3)
  })
})

describe("parseOptions squashMerge", () => {
  test("reads a boolean", () => {
    expect(parseOptions({ squashMerge: true }).squashMerge).toBe(true)
    expect(parseOptions({ squashMerge: false }).squashMerge).toBe(false)
  })

  test("falls back to true on wrong type", () => {
    expect(parseOptions({ squashMerge: "false" }).squashMerge).toBe(true)
    expect(parseOptions({ squashMerge: 0 }).squashMerge).toBe(true)
  })
})

describe("parseOptions agent overrides", () => {
  test("reads a string agent, including empty", () => {
    expect(parseOptions({ intakeAgent: "plan" }).intakeAgent).toBe("plan")
    expect(parseOptions({ validatorAgent: "reviewer" }).validatorAgent).toBe("reviewer")
    expect(parseOptions({ intakeAgent: "" }).intakeAgent).toBe("")
  })

  test("falls back to undefined on wrong type", () => {
    expect(parseOptions({ intakeAgent: 5 }).intakeAgent).toBeUndefined()
    expect(parseOptions({ validatorAgent: {} }).validatorAgent).toBeUndefined()
  })
})

describe("parseOptions passthrough", () => {
  test("resolves a full valid options object and ignores keys owned by other readers", () => {
    expect(
      parseOptions({
        inProgressLimit: 4,
        helperRetries: 2,
        sendBackStopThreshold: 5,
        squashMerge: false,
        intakeAgent: "plan",
        validatorAgent: "reviewer",
        validatorModels: [{ providerID: "openai", modelID: "gpt-5" }],
        commands: { check: "bun run test" },
      }),
    ).toEqual({
      inProgressLimit: 4,
      helperRetries: 2,
      sendBackStopThreshold: 5,
      squashMerge: false,
      intakeAgent: "plan",
      validatorAgent: "reviewer",
    })
  })
})
