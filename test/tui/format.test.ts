import { describe, expect, test } from "bun:test"
import {
  type Badge,
  confidenceBar,
  formatAge,
  formatDiff,
  formatModeRationale,
  gateBadges,
  shortSubtaskTitle,
  summarizeSubtasks,
} from "../../src/tui/format"

describe("formatAge", () => {
  test("renders sub-minute as <1m", () => {
    const now = 1_000_000
    expect(formatAge(now - 30_000, now)).toBe("<1m")
  })

  test("renders minutes, hours, and days", () => {
    const now = 1_000_000
    expect(formatAge(now - 2 * 60_000, now)).toBe("2m")
    expect(formatAge(now - 2 * 60 * 60_000, now)).toBe("2h")
    expect(formatAge(now - 2 * 24 * 60 * 60_000, now)).toBe("2d")
  })

  test("clamps negative diffs to <1m", () => {
    const now = 1_000_000
    expect(formatAge(now + 60_000, now)).toBe("<1m")
  })
})

describe("formatDiff", () => {
  test("formats additions, deletions, and files", () => {
    expect(formatDiff({ additions: 12, deletions: 3, files: 2 })).toBe("+12 -3 · 2 files")
  })

  test("uses singular file", () => {
    expect(formatDiff({ additions: 1, deletions: 0, files: 1 })).toBe("+1 -0 · 1 file")
  })

  test("returns undefined when summary is absent", () => {
    expect(formatDiff(undefined)).toBeUndefined()
  })
})

describe("confidenceBar", () => {
  test("fills the bar completely at confidence 10", () => {
    expect(confidenceBar(10)).toBe("██████████")
  })

  test("fills proportionally for a partial confidence", () => {
    expect(confidenceBar(5)).toBe("█████░░░░░")
    expect(confidenceBar(2)).toBe("██░░░░░░░░")
  })

  test("is entirely empty at confidence 0", () => {
    expect(confidenceBar(0)).toBe("░░░░░░░░░░")
  })

  test("is entirely empty when unscored", () => {
    expect(confidenceBar(undefined)).toBe("░░░░░░░░░░")
  })

  test("clamps out-of-range confidence instead of producing a malformed bar", () => {
    expect(confidenceBar(15)).toBe("██████████")
    expect(confidenceBar(-3)).toBe("░░░░░░░░░░")
  })

  test("honors a custom width", () => {
    expect(confidenceBar(5, 4)).toBe("██░░")
  })
})

describe("gateBadges", () => {
  test.each<[string, Record<string, unknown>, Badge[]]>([
    [
      "needs-you badge when a permission is pending",
      { status: "in_progress", awaitingPermissions: [{ id: "p1", title: "Run rm -rf?", sessionID: "s1" }] },
      [{ text: "△ needs you", tone: "warning" }],
    ],
    [
      "needs-you badge is placed first, ahead of other badges",
      {
        status: "backlog",
        boardTask: true,
        intakeOutcome: "ran",
        awaitingPermissions: [{ id: "p1", title: "x", sessionID: "s1" }],
      },
      [
        { text: "△ needs you", tone: "warning" },
        { text: "intake ok", tone: "success" },
      ],
    ],
    [
      "needs-you badge counts multiple waiting permissions",
      {
        status: "in_progress",
        awaitingPermissions: [
          { id: "p1", title: "a", sessionID: "s1" },
          { id: "p2", title: "b", sessionID: "s2" },
        ],
      },
      [{ text: "△ 2 need you", tone: "warning" }],
    ],
    [
      "intake ok once a backlog board task is intake-ready",
      { status: "backlog", boardTask: true, intakeOutcome: "ran" },
      [{ text: "intake ok", tone: "success" }],
    ],
    [
      "intake… while a backlog board task is still preparing intake",
      { status: "backlog", boardTask: true, intakeOutcome: "pending" },
      [{ text: "intake…", tone: "muted" }],
    ],
    ["no intake badge for a non-board backlog session", { status: "backlog", intakeOutcome: "pending" }, []],
    [
      "intake failed when a failed intake would otherwise read as ready",
      { status: "backlog", boardTask: true, intakeOutcome: "failed" },
      [{ text: "intake failed", tone: "error" }],
    ],
    [
      "intake failed while an intake helperError is recorded even if the outcome has not flipped yet",
      {
        status: "backlog",
        boardTask: true,
        intakeOutcome: "pending",
        helperError: { role: "intake", message: "boom" },
      },
      [{ text: "intake failed", tone: "error" }],
    ],
    [
      "setup ok when the setup command succeeded",
      { status: "backlog", setup: { command: "t", exitCode: 0, output: "ok" } },
      [{ text: "setup ok", tone: "success" }],
    ],
    [
      "setup failed when the setup command failed",
      { status: "backlog", setup: { command: "t", exitCode: 1, output: "fail" } },
      [{ text: "setup failed", tone: "error" }],
    ],
    [
      "places the setup badge before the check badge",
      {
        status: "review",
        setup: { command: "t", exitCode: 0, output: "ok" },
        check: { command: "t", exitCode: 0, output: "ok" },
      },
      [
        { text: "setup ok", tone: "success" },
        { text: "check ok", tone: "success" },
      ],
    ],
    [
      "check ok when the deterministic check passed",
      { status: "review", check: { command: "t", exitCode: 0, output: "ok" } },
      [{ text: "check ok", tone: "success" }],
    ],
    [
      "check failed when the deterministic check failed",
      { status: "review", check: { command: "t", exitCode: 1, output: "fail" } },
      [{ text: "check failed", tone: "error" }],
    ],
    [
      "check failed when the deterministic check did not complete",
      { status: "review", check: { command: "t", exitCode: null, output: "timeout" } },
      [{ text: "check failed", tone: "error" }],
    ],
    [
      "check skipped when every configured check was out of scope",
      {
        status: "review",
        check: {
          command: "alpha: npm test",
          exitCode: 0,
          output: "alpha: skipped",
          steps: [
            {
              name: "alpha",
              cwd: "project-alpha",
              command: "npm test",
              status: "skipped",
              exitCode: null,
              output: "",
              reason: "no changed files in scope",
            },
          ],
        },
      },
      [{ text: "check skipped", tone: "muted" }],
    ],
    [
      "check partial when some configured checks ran and others skipped",
      {
        status: "review",
        check: {
          command: "alpha: npm test && beta: npm test",
          exitCode: 0,
          output: "alpha ok",
          steps: [
            { name: "alpha", cwd: "project-alpha", command: "npm test", status: "ran", exitCode: 0, output: "ok" },
            { name: "beta", cwd: "project-beta", command: "npm test", status: "skipped", exitCode: null, output: "" },
          ],
        },
      },
      [{ text: "check partial", tone: "success" }],
    ],
    [
      "setup skipped when every configured setup step was out of scope",
      {
        status: "backlog",
        setup: {
          command: "alpha: npm ci",
          exitCode: 0,
          output: "alpha: skipped",
          steps: [
            {
              name: "alpha",
              cwd: "project-alpha",
              command: "npm ci",
              status: "skipped",
              exitCode: null,
              output: "",
            },
          ],
        },
      },
      [{ text: "setup skipped", tone: "muted" }],
    ],
    [
      "setup partial when some configured setup steps ran and others skipped",
      {
        status: "backlog",
        setup: {
          command: "alpha: npm ci && beta: npm ci",
          exitCode: 0,
          output: "alpha ok",
          steps: [
            { name: "alpha", cwd: "project-alpha", command: "npm ci", status: "ran", exitCode: 0, output: "ok" },
            { name: "beta", cwd: "project-beta", command: "npm ci", status: "skipped", exitCode: null, output: "" },
          ],
        },
      },
      [{ text: "setup partial", tone: "success" }],
    ],
    [
      "places the check badge before the validator/findings badge",
      { status: "review", check: { command: "t", exitCode: 0, output: "ok" }, validatorOutcome: "ran" },
      [
        { text: "check ok", tone: "success" },
        { text: "findings clear", tone: "success" },
      ],
    ],
    [
      "reviewing… while the validator child is running (outcome pending)",
      { status: "review", validatorSessionID: "v1", validatorOutcome: "pending" },
      [{ text: "reviewing…", tone: "muted" }],
    ],
    [
      "review failed when the validator failed to spawn",
      { status: "review", validatorOutcome: "failed" },
      [{ text: "review failed", tone: "error" }],
    ],
    [
      "review failed while a validator helperError is recorded even if the outcome has not flipped yet",
      { status: "review", helperError: { role: "validator", message: "boom" } },
      [{ text: "review failed", tone: "error" }],
    ],
    [
      "findings clear when the validator ran clean",
      { status: "review", validatorOutcome: "ran" },
      [{ text: "findings clear", tone: "success" }],
    ],
    [
      "N to triage counts pending findings after the validator ran",
      { status: "review", validatorOutcome: "ran", findings: [{ id: "f1", summary: "issue" }] },
      [{ text: "1 to triage", tone: "warning" }],
    ],
    [
      "no findings badge until the validator has run",
      { status: "review", findings: [{ id: "f1", summary: "issue" }] },
      [],
    ],
    [
      "iter N surfaces send-back generations",
      { status: "in_progress", generation: 2 },
      [{ text: "iter 2", tone: "muted" }],
    ],
    [
      "iter badge warns at the send-back stop threshold",
      { status: "in_progress", generation: 3 },
      [{ text: "iter 3", tone: "warning" }],
    ],
    [
      "iter badge warns above the send-back stop threshold",
      { status: "in_progress", generation: 4 },
      [{ text: "iter 4", tone: "warning" }],
    ],
    ["approved for an approved task", { status: "review", approved: true }, [{ text: "approved", tone: "success" }]],
    [
      "composes validator, findings, iteration, and approval in order",
      {
        status: "review",
        validatorOutcome: "ran",
        findings: [{ id: "f1", summary: "issue" }],
        generation: 2,
        approved: true,
      },
      [
        { text: "1 to triage", tone: "warning" },
        { text: "iter 2", tone: "muted" },
        { text: "approved", tone: "success" },
      ],
    ],
    ["returns an empty list when nothing is worth badging", { status: "in_progress" }, []],
    [
      "shows a muted mode badge for autonomous (shortened to auto)",
      {
        intake: {
          understanding: "x",
          decisions: [],
          mode: { recommended: "autonomous", rationale: "A cheap check catches every wrong answer." },
        },
      },
      [{ text: "mode: auto", tone: "muted" }],
    ],
    [
      "shows a muted mode badge for assisted",
      {
        intake: {
          understanding: "x",
          decisions: [],
          mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
        },
      },
      [{ text: "mode: assisted", tone: "muted" }],
    ],
    [
      "shows a muted mode badge for manual",
      {
        intake: {
          understanding: "x",
          decisions: [],
          mode: { recommended: "manual", rationale: "The human must understand every line of the change." },
        },
      },
      [{ text: "mode: manual", tone: "muted" }],
    ],
    ["omits the mode badge when the mode is invalid", { intake: { understanding: "x", decisions: [] } }, []],
  ])("%s", (_name, kagan, expected) => {
    expect(gateBadges({ kagan })).toEqual(expected)
  })

  test("iter badge respects a custom threshold", () => {
    expect(gateBadges({ kagan: { status: "in_progress", generation: 2 } }, 2)).toEqual([
      { text: "iter 2", tone: "warning" },
    ])
  })
})

describe("shortSubtaskTitle", () => {
  test("strips the subagent parenthetical from the displayed title", () => {
    expect(shortSubtaskTitle({ title: "Review i18n a11y (@general subagent)", slug: "review-i18n" })).toBe(
      "Review i18n a11y",
    )
  })
})

describe("formatModeRationale", () => {
  test("returns undefined when there is no mode", () => {
    expect(formatModeRationale({ kagan: {} })).toBeUndefined()
    expect(formatModeRationale(undefined)).toBeUndefined()
  })

  test("returns the rationale without overlay when a check command is configured", () => {
    expect(
      formatModeRationale(
        {
          kagan: {
            intake: {
              understanding: "x",
              decisions: [],
              mode: { recommended: "assisted", rationale: "High blast radius and no trusted automatic check." },
            },
          },
        },
        "bun test",
      ),
    ).toBe("High blast radius and no trusted automatic check.")
  })

  test("appends the no-check overlay when no check command is configured", () => {
    expect(
      formatModeRationale(
        {
          kagan: {
            intake: {
              understanding: "x",
              decisions: [],
              mode: { recommended: "manual", rationale: "Novel territory that needs careful human review." },
            },
          },
        },
        undefined,
      ),
    ).toBe("Novel territory that needs careful human review. (no automatic check configured - lean assisted)")
  })

  test("appends the no-check overlay when a check command summary is blank", () => {
    expect(
      formatModeRationale(
        {
          kagan: {
            intake: {
              understanding: "x",
              decisions: [],
              mode: { recommended: "autonomous", rationale: "A trusted automatic check catches every wrong answer." },
            },
          },
        },
        "",
      ),
    ).toBe("A trusted automatic check catches every wrong answer. (no automatic check configured - lean assisted)")
  })
})

describe("gateBadges — mode", () => {
  test("shows a muted mode badge for autonomous (shortened to auto)", () => {
    expect(
      gateBadges({
        kagan: {
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "autonomous", rationale: "A cheap check catches every wrong answer." },
          },
        },
      }),
    ).toEqual([{ text: "mode: auto", tone: "muted" }])
  })

  test("shows a muted mode badge for assisted", () => {
    expect(
      gateBadges({
        kagan: {
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "assisted", rationale: "No trusted check and the blast radius is high." },
          },
        },
      }),
    ).toEqual([{ text: "mode: assisted", tone: "muted" }])
  })

  test("shows a muted mode badge for manual", () => {
    expect(
      gateBadges({
        kagan: {
          intake: {
            understanding: "x",
            decisions: [],
            mode: { recommended: "manual", rationale: "The human must understand every line of the change." },
          },
        },
      }),
    ).toEqual([{ text: "mode: manual", tone: "muted" }])
  })

  test("omits the mode badge when the mode is invalid", () => {
    expect(gateBadges({ kagan: { intake: { understanding: "x", decisions: [] } } })).toEqual([])
  })
})

describe("summarizeSubtasks", () => {
  test("summarizes a single subtask", () => {
    expect(summarizeSubtasks([{ title: "Review infra (@general subagent)", slug: "a" }])).toBe(
      "1 subtask · Review infra",
    )
  })

  test("summarizes multiple subtasks with overflow", () => {
    expect(
      summarizeSubtasks([
        { title: "A", slug: "a" },
        { title: "B", slug: "b" },
        { title: "C", slug: "c" },
        { title: "D", slug: "d" },
      ]),
    ).toBe("4 subtasks · A, B, C, +1")
  })
})
