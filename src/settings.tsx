/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { useKeyboard, useTerminalDimensions } from "@opentui/solid"
import { For, Show, createMemo, createSignal } from "solid-js"
import { readFile, writeFile } from "node:fs/promises"
import { join } from "node:path"
import {
  commandPlan,
  helperRetries,
  inProgressCap,
  sendBackStopThreshold,
  squashMerge,
  validateCommandPlan,
} from "./task"
import { SETTINGS_ROUTE, ROUTE } from "./types"

type Section = "General" | "Agents" | "Commands" | "Validator models" | "JSON preview"
type Draft = {
  inProgressLimit: number
  helperRetries: number
  sendBackStopThreshold: number
  squashMerge: boolean
  intakeAgent: string
  validatorAgent: string
  validatorModels: unknown[]
  commands: {
    setup: unknown[]
    check: unknown[]
  }
}

const SECTIONS: Section[] = ["General", "Agents", "Commands", "Validator models", "JSON preview"]

function draftFromOptions(options?: Record<string, unknown>): Draft {
  return {
    inProgressLimit: inProgressCap(options),
    helperRetries: helperRetries(options),
    sendBackStopThreshold: sendBackStopThreshold(options),
    squashMerge: squashMerge(options),
    intakeAgent: typeof options?.intakeAgent === "string" ? options.intakeAgent : "",
    validatorAgent: typeof options?.validatorAgent === "string" ? options.validatorAgent : "",
    validatorModels: Array.isArray(options?.validatorModels) ? options.validatorModels : [],
    commands: {
      setup: commandPlan(options, "setup"),
      check: commandPlan(options, "check"),
    },
  }
}

function optionsFromDraft(draft: Draft): Record<string, unknown> {
  const options: Record<string, unknown> = {
    inProgressLimit: draft.inProgressLimit,
    helperRetries: draft.helperRetries,
    sendBackStopThreshold: draft.sendBackStopThreshold,
    squashMerge: draft.squashMerge,
    commands: draft.commands,
  }
  if (draft.intakeAgent.trim()) options.intakeAgent = draft.intakeAgent.trim()
  if (draft.validatorAgent.trim()) options.validatorAgent = draft.validatorAgent.trim()
  if (draft.validatorModels.length > 0) options.validatorModels = draft.validatorModels
  return options
}

function pluginOptionsJson(draft: Draft): string {
  return JSON.stringify(optionsFromDraft(draft), null, 2)
}

function validateValidatorModels(value: unknown): string | undefined {
  if (!Array.isArray(value)) return "validatorModels must be a JSON array"
  for (let i = 0; i < value.length; i++) {
    const item = value[i]
    if (
      typeof item !== "object" ||
      item === null ||
      typeof item.providerID !== "string" ||
      typeof item.modelID !== "string"
    ) {
      return `validatorModels[${i}] must be { providerID: string, modelID: string }`
    }
  }
  return undefined
}

function validateDraft(draft: Draft): string | undefined {
  if (draft.inProgressLimit < 1) return "inProgressLimit must be at least 1"
  if (draft.helperRetries < 0) return "helperRetries must be at least 0"
  if (draft.sendBackStopThreshold < 1) return "sendBackStopThreshold must be at least 1"
  const setupError = validateCommandPlan(draft.commands.setup, "setup")
  if (setupError) return setupError
  const checkError = validateCommandPlan(draft.commands.check, "check")
  if (checkError) return checkError
  return validateValidatorModels(draft.validatorModels)
}

async function saveOptions(worktree: string, draft: Draft): Promise<string> {
  const error = validateDraft(draft)
  if (error) throw new Error(error)
  const path = join(worktree, "opencode.json")
  const raw = await readFile(path, "utf8")
  const config = JSON.parse(raw) as { plugin?: unknown }
  if (!Array.isArray(config.plugin)) throw new Error("opencode.json has no plugin array")
  const index = config.plugin.findIndex((entry) => {
    const path = Array.isArray(entry) ? entry[0] : entry
    return typeof path === "string" && path.includes("kagan")
  })
  if (index === -1) throw new Error("opencode.json has no Kagan plugin entry")
  const entry = config.plugin[index]
  const pathValue = Array.isArray(entry) ? entry[0] : entry
  config.plugin[index] = [pathValue, optionsFromDraft(draft)]
  await writeFile(path, `${JSON.stringify(config, null, 2)}\n`)
  return "Saved opencode.json. Restart OpenCode or reopen the project to apply changes."
}

type Row = { label: string; value: string; edit?: () => void }

function parseJsonArray(value: string): unknown[] {
  const parsed = JSON.parse(value)
  if (!Array.isArray(parsed)) throw new Error("Value must be a JSON array")
  return parsed
}

function rowsFor(
  section: Section,
  draft: Draft,
  setDraft: (draft: Draft) => void,
  setMessage: (message: string | undefined) => void,
  api: TuiPluginApi,
): Row[] {
  const prompt = (title: string, value: string, onConfirm: (value: string) => void) => {
    api.ui.dialog.replace(() => (
      <api.ui.DialogPrompt
        title={title}
        value={value}
        placeholder={title}
        onConfirm={(next) => {
          api.ui.dialog.clear()
          onConfirm(next)
        }}
        onCancel={() => api.ui.dialog.clear()}
      />
    ))
  }

  const editJsonArray = (
    title: string,
    value: string,
    setter: (parsed: unknown[]) => void,
    validate?: (parsed: unknown[]) => string | undefined,
  ) => {
    prompt(title, value, (next) => {
      let parsed: unknown[]
      try {
        parsed = parseJsonArray(next)
      } catch (error) {
        setMessage(error instanceof Error ? error.message : String(error))
        return
      }
      if (validate) {
        const error = validate(parsed)
        if (error) {
          setMessage(error)
          return
        }
      }
      setter(parsed)
    })
  }

  if (section === "General") {
    const number = (
      key: "inProgressLimit" | "helperRetries" | "sendBackStopThreshold",
      label: string,
      min: number,
    ) => ({
      label,
      value: String(draft[key]),
      edit: () =>
        prompt(label, String(draft[key]), (value) => {
          const parsed = Number(value)
          if (Number.isInteger(parsed) && parsed >= min) setDraft({ ...draft, [key]: parsed })
        }),
    })
    return [
      number("inProgressLimit", "inProgressLimit", 1),
      number("helperRetries", "helperRetries", 0),
      number("sendBackStopThreshold", "sendBackStopThreshold", 1),
      {
        label: "squashMerge",
        value: draft.squashMerge ? "yes" : "no",
        edit: () => setDraft({ ...draft, squashMerge: !draft.squashMerge }),
      },
    ]
  }
  if (section === "Agents") {
    return [
      {
        label: "intakeAgent",
        value: draft.intakeAgent || "session default",
        edit: () =>
          prompt("intakeAgent", draft.intakeAgent, (value) => setDraft({ ...draft, intakeAgent: value.trim() })),
      },
      {
        label: "validatorAgent",
        value: draft.validatorAgent || "session default",
        edit: () =>
          prompt("validatorAgent", draft.validatorAgent, (value) =>
            setDraft({ ...draft, validatorAgent: value.trim() }),
          ),
      },
    ]
  }
  if (section === "Commands") {
    return [
      {
        label: "setup",
        value: `${draft.commands.setup.length} command(s)`,
        edit: () =>
          editJsonArray(
            "setup commands JSON",
            JSON.stringify(draft.commands.setup, null, 2),
            (parsed) => setDraft({ ...draft, commands: { ...draft.commands, setup: parsed } }),
            (parsed) => validateCommandPlan(parsed, "setup"),
          ),
      },
      {
        label: "check",
        value: `${draft.commands.check.length} command(s)`,
        edit: () =>
          editJsonArray(
            "check commands JSON",
            JSON.stringify(draft.commands.check, null, 2),
            (parsed) => setDraft({ ...draft, commands: { ...draft.commands, check: parsed } }),
            (parsed) => validateCommandPlan(parsed, "check"),
          ),
      },
    ]
  }
  if (section === "Validator models") {
    return [
      {
        label: "validatorModels",
        value: `${draft.validatorModels.length} model(s)`,
        edit: () =>
          editJsonArray(
            "validatorModels JSON",
            JSON.stringify(draft.validatorModels, null, 2),
            (parsed) => setDraft({ ...draft, validatorModels: parsed }),
            validateValidatorModels,
          ),
      },
    ]
  }
  return [{ label: "plugin options", value: pluginOptionsJson(draft) }]
}

export function Settings(props: { api: TuiPluginApi; options?: Record<string, unknown> }) {
  const dimensions = useTerminalDimensions()
  const theme = () => props.api.theme.current
  const [draft, setDraft] = createSignal(draftFromOptions(props.options))
  const [sectionIndex, setSectionIndex] = createSignal(0)
  const [rowIndex, setRowIndex] = createSignal(0)
  const [message, setMessage] = createSignal<string>()
  const section = () => SECTIONS[sectionIndex()] ?? "General"
  const rows = createMemo(() => rowsFor(section(), draft(), setDraft, setMessage, props.api))
  const selectedRow = () => rows()[rowIndex()]

  useKeyboard((key) => {
    if (props.api.route.current.name !== SETTINGS_ROUTE || props.api.ui.dialog.open) return
    if (key.name === "escape" || key.name === "q") {
      props.api.route.navigate(ROUTE)
      return
    }
    if (key.name === "tab" || key.name === "right") {
      setSectionIndex((index) => (index + 1) % SECTIONS.length)
      setRowIndex(0)
      return
    }
    if (key.name === "left") {
      setSectionIndex((index) => (index + SECTIONS.length - 1) % SECTIONS.length)
      setRowIndex(0)
      return
    }
    if (key.name === "down" || key.name === "j") {
      setRowIndex((index) => Math.min(index + 1, rows().length - 1))
      return
    }
    if (key.name === "up" || key.name === "k") {
      setRowIndex((index) => Math.max(index - 1, 0))
      return
    }
    if (key.name === "return" || key.name === "e") {
      try {
        selectedRow()?.edit?.()
      } catch (error) {
        setMessage(error instanceof Error ? error.message : String(error))
      }
      return
    }
    if (key.name === "s") {
      void saveOptions(props.api.state.path.worktree, draft()).then(setMessage, (error) =>
        setMessage(error instanceof Error ? error.message : String(error)),
      )
    }
  })

  return (
    <box position="absolute" left={0} top={0} width={dimensions().width} height={dimensions().height} padding={1}>
      <box flexDirection="column" width="100%" height="100%" borderColor={theme().border}>
        <box flexDirection="row" justifyContent="space-between" flexShrink={0}>
          <text fg={theme().text} attributes={TextAttributes.BOLD}>
            Kagan settings
          </text>
          <text fg={theme().textMuted}>q/esc back</text>
        </box>
        <box flexDirection="row" flexGrow={1} minHeight={0} gap={2}>
          <box width={22} flexDirection="column" border={["right"]} borderColor={theme().border}>
            <For each={SECTIONS}>
              {(item, index) => <text fg={index() === sectionIndex() ? theme().primary : theme().text}>{item}</text>}
            </For>
          </box>
          <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
            <box flexDirection="column" gap={1}>
              <Show
                when={section() !== "JSON preview"}
                fallback={<text wrapMode="word">{pluginOptionsJson(draft())}</text>}
              >
                <For each={rows()}>
                  {(row, index) => (
                    <box
                      flexDirection="row"
                      gap={2}
                      backgroundColor={index() === rowIndex() ? theme().primary : undefined}
                    >
                      <text width={24} fg={index() === rowIndex() ? theme().selectedListItemText : theme().textMuted}>
                        {row.label}
                      </text>
                      <text fg={index() === rowIndex() ? theme().selectedListItemText : theme().text}>{row.value}</text>
                    </box>
                  )}
                </For>
              </Show>
            </box>
          </scrollbox>
        </box>
        <box flexDirection="row" justifyContent="space-between" flexShrink={0}>
          <text fg={theme().textMuted}>{message() ?? "enter/e edit   s save   tab switch section"}</text>
          <text fg={theme().textMuted}>opencode.json only</text>
        </box>
      </box>
    </box>
  )
}
