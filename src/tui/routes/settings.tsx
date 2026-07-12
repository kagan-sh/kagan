/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { For, Show, createMemo, createSignal } from "solid-js"
import { readFile, writeFile } from "node:fs/promises"
import { join } from "node:path"
import { OptionBoundsSchema } from "../../domain/options"
import { commandPlan } from "../../domain/task/commands"
import { helperRetries, inProgressCap, sendBackStopThreshold, squashMerge } from "../../domain/task/policy"
import type { CommandSpec, ModelRef } from "../../domain/task/types"
import { SETTINGS_ROUTE, ROUTE } from "../types"
import { useKeyIntercept } from "../renderer"
import { openCommandListEditor, openValidatorModelListEditor } from "./settings-list-editor"

type Section = "General" | "Agents" | "Commands" | "Validator models" | "JSON preview"

type Draft = {
  inProgressLimit: number
  helperRetries: number
  sendBackStopThreshold: number
  squashMerge: boolean
  intakeAgent: string
  validatorAgent: string
  validatorModels: ModelRef[]
  commands: {
    setup: CommandSpec[]
    check: CommandSpec[]
  }
}

const SECTIONS: Section[] = ["General", "Agents", "Commands", "Validator models", "JSON preview"]

function modelRef(value: unknown): ModelRef | undefined {
  if (typeof value !== "object" || value === null) return undefined
  const raw = value as Record<string, unknown>
  const providerID = typeof raw.providerID === "string" ? raw.providerID.trim() : ""
  const modelID = typeof raw.modelID === "string" ? raw.modelID.trim() : ""
  if (!providerID || !modelID) return undefined
  return { providerID, modelID }
}

function draftFromOptions(options?: Record<string, unknown>): Draft {
  return {
    inProgressLimit: inProgressCap(options),
    helperRetries: helperRetries(options),
    sendBackStopThreshold: sendBackStopThreshold(options),
    squashMerge: squashMerge(options),
    intakeAgent: typeof options?.intakeAgent === "string" ? options.intakeAgent : "",
    validatorAgent: typeof options?.validatorAgent === "string" ? options.validatorAgent : "",
    validatorModels: Array.isArray(options?.validatorModels)
      ? options.validatorModels.map(modelRef).filter((model): model is ModelRef => model !== undefined)
      : [],
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
  }
  if (draft.intakeAgent.trim()) options.intakeAgent = draft.intakeAgent.trim()
  if (draft.validatorAgent.trim()) options.validatorAgent = draft.validatorAgent.trim()
  if (draft.validatorModels.length > 0) options.validatorModels = draft.validatorModels
  if (draft.commands.setup.length > 0 || draft.commands.check.length > 0) {
    options.commands = draft.commands
  }
  return options
}

function pluginOptionsJson(draft: Draft): string {
  return JSON.stringify(optionsFromDraft(draft), null, 2)
}

function validateValidatorModels(value: ModelRef[]): string | undefined {
  for (let i = 0; i < value.length; i++) {
    const item = value[i]
    if (item === undefined) continue
    if (!item.providerID.trim() || !item.modelID.trim()) {
      return `validatorModels[${i}] must be { providerID: string, modelID: string }`
    }
  }
  return undefined
}

function validateDraft(draft: Draft): string | undefined {
  const modelError = validateValidatorModels(draft.validatorModels)
  if (modelError) return modelError
  const bounds = OptionBoundsSchema.safeParse({
    inProgressLimit: draft.inProgressLimit,
    helperRetries: draft.helperRetries,
    sendBackStopThreshold: draft.sendBackStopThreshold,
  })
  if (bounds.success) return undefined
  const field = bounds.error.issues[0]?.path[0]
  if (field === "inProgressLimit") return "inProgressLimit must be at least 1"
  if (field === "helperRetries") return "helperRetries must be at least 0"
  if (field === "sendBackStopThreshold") return "sendBackStopThreshold must be at least 1"
  return bounds.error.issues[0]?.message ?? "Invalid settings"
}

async function saveOptions(worktree: string, draft: Draft): Promise<string> {
  const error = validateDraft(draft)
  if (error) throw new Error(error)
  const path = join(worktree, "opencode.json")
  let raw: string
  try {
    raw = await readFile(path, "utf8")
  } catch (error) {
    if (error && typeof error === "object" && "code" in error && (error as { code: string }).code === "ENOENT") {
      throw new Error("opencode.json not found in project root")
    }
    throw error
  }
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
          openCommandListEditor(api, "setup", draft.commands.setup, (setup) =>
            setDraft({ ...draft, commands: { ...draft.commands, setup } }),
          ),
      },
      {
        label: "check",
        value: `${draft.commands.check.length} command(s)`,
        edit: () =>
          openCommandListEditor(api, "check", draft.commands.check, (check) =>
            setDraft({ ...draft, commands: { ...draft.commands, check } }),
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
          openValidatorModelListEditor(api, draft.validatorModels, (models) =>
            setDraft({ ...draft, validatorModels: models }),
          ),
      },
    ]
  }
  return [{ label: "plugin options", value: pluginOptionsJson(draft) }]
}

export function Settings(props: { api: TuiPluginApi; options?: Record<string, unknown> }) {
  const theme = () => props.api.theme.current
  const [draft, setDraft] = createSignal(draftFromOptions(props.options))
  const [sectionIndex, setSectionIndex] = createSignal(0)
  const [rowIndex, setRowIndex] = createSignal(0)
  const [message, setMessage] = createSignal<string>()
  const section = () => SECTIONS[sectionIndex()] ?? "General"
  const rows = createMemo(() => rowsFor(section(), draft(), setDraft, setMessage, props.api))
  const selectedRow = () => rows()[rowIndex()]

  useKeyIntercept(props.api, (key) => {
    if (props.api.route.current.name !== SETTINGS_ROUTE || props.api.ui.dialog.open) return false
    if (key.name === "escape" || key.name === "q") {
      props.api.route.navigate(ROUTE)
      return true
    }
    if (key.name === "tab" || key.name === "right") {
      setSectionIndex((index) => (index + 1) % SECTIONS.length)
      setRowIndex(0)
      return true
    }
    if (key.name === "left") {
      setSectionIndex((index) => (index + SECTIONS.length - 1) % SECTIONS.length)
      setRowIndex(0)
      return true
    }
    if (key.name === "down" || key.name === "j") {
      setRowIndex((index) => Math.min(index + 1, rows().length - 1))
      return true
    }
    if (key.name === "up" || key.name === "k") {
      setRowIndex((index) => Math.max(index - 1, 0))
      return true
    }
    if (key.name === "return" || key.name === "e") {
      try {
        selectedRow()?.edit?.()
      } catch (error) {
        setMessage(error instanceof Error ? error.message : String(error))
      }
      return true
    }
    if (key.name === "s") {
      void saveOptions(props.api.state.path.worktree, draft()).then(setMessage, (error) =>
        setMessage(error instanceof Error ? error.message : String(error)),
      )
      return true
    }
    return false
  })

  return (
    <box position="absolute" left={0} top={0} width="100%" height="100%" padding={1}>
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
