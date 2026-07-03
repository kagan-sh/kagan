/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { useKeyboard } from "@opentui/solid"
import { createSignal, For, onMount } from "solid-js"

const SEEN_KEY = "kagan:onboarding"

const LOGO_LINES = ["█▄▀  ▄▀▄  █▀▀  ▄▀▄  █▄  █", "█▀▄  █▀█  █▄█  █▀█  █ ▀▄█"]

const STEPS = [
  {
    title: "Supervised tasks",
    lines: [
      "Every card is an agent session working in its own git worktree on a kagan/<slug> branch — your checkout is never touched.",
      "Cards move Backlog → In Progress → Review → Done, with a gate at every move.",
    ],
  },
  {
    title: "Create & intake",
    lines: [
      "Press n to create a task: title, description, model, and base branch.",
      "A read-only task-prep agent analyzes it first. Approve or override its assumptions; the card shows intake ok once it's ready to start.",
    ],
  },
  {
    title: "Start & review",
    lines: [
      "Press m to start: the agent works from the intake's refined instruction, within the configured running-task cap.",
      "When it finishes, it moves to Review automatically — a reviewer agent files findings against the original task, ranked by confidence.",
    ],
  },
  {
    title: "Triage & merge",
    lines: [
      "Press a to triage: rule every finding ignore, intended, or clarify — then choose where to merge, or not at all.",
      "Press s to send a task back for another iteration in the same worktree. Press ? anytime for all keys.",
    ],
  },
]

export function Onboarding(props: { api: TuiPluginApi }) {
  const theme = () => props.api.theme.current
  const [step, setStep] = createSignal(0)
  const last = STEPS.length - 1

  onMount(() => props.api.ui.dialog.setSize("medium"))

  const close = () => props.api.ui.dialog.clear()
  const dismissForever = () => {
    props.api.kv.set(SEEN_KEY, true)
    close()
  }

  useKeyboard((key) => {
    if (key.name === "left" || key.name === "h") {
      setStep((current) => Math.max(0, current - 1))
      return
    }
    if (key.name === "right" || key.name === "l") {
      setStep((current) => Math.min(last, current + 1))
      return
    }
    if (key.name === "return") {
      if (step() === last) close()
      else setStep((current) => current + 1)
      return
    }
    if (key.name === "x") dismissForever()
  })

  return (
    <box paddingLeft={2} paddingRight={2} gap={1}>
      <box flexDirection="column" alignItems="center">
        <For each={LOGO_LINES}>{(line) => <text fg={theme().primary}>{line}</text>}</For>
      </box>
      <box flexDirection="column" alignItems="center">
        <text fg={theme().text} attributes={TextAttributes.BOLD}>
          Welcome to the board!
        </text>
      </box>
      <box flexDirection="column" gap={1}>
        <text fg={theme().accent} attributes={TextAttributes.BOLD}>
          {STEPS[step()]!.title}
        </text>
        <For each={STEPS[step()]!.lines}>
          {(line) => (
            <text fg={theme().text} wrapMode="word">
              {line}
            </text>
          )}
        </For>
      </box>
      <box flexDirection="row" justifyContent="center" gap={1}>
        <For each={STEPS}>
          {(_, index) => <text fg={index() === step() ? theme().primary : theme().textMuted}>●</text>}
        </For>
      </box>
      <box paddingBottom={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          ←/→ <span style={{ fg: theme().textMuted }}>steps</span>
        </text>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>{step() === last ? "finish" : "next"}</span>
        </text>
        <text fg={theme().text}>
          esc <span style={{ fg: theme().textMuted }}>dismiss</span>
        </text>
        <text fg={theme().text}>
          x <span style={{ fg: theme().textMuted }}>don't show again</span>
        </text>
      </box>
    </box>
  )
}

// Keyed by api instance rather than a module boolean so parallel consumers of this
// module (bun test shares it across suites) each get their own show-once state.
const shownThisRun = new WeakSet<TuiPluginApi>()

export function showOnboarding(api: TuiPluginApi): void {
  api.ui.dialog.replace(() => <Onboarding api={api} />)
}

export function maybeShowOnboarding(api: TuiPluginApi): boolean {
  if (shownThisRun.has(api) || api.kv.get(SEEN_KEY, false)) return false
  shownThisRun.add(api)
  showOnboarding(api)
  return true
}
