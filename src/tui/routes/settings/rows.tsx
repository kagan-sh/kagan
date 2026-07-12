/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { openCommandListEditor, openValidatorModelListEditor } from "./list-editor"
import type { Draft, Section } from "./draft"
import { pluginOptionsJson } from "./draft"

export type Row = { label: string; value: string; edit?: () => void }

function settingsPrompt(api: TuiPluginApi, title: string, value: string, onConfirm: (value: string) => void) {
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

function generalRows(draft: Draft, setDraft: (draft: Draft) => void, api: TuiPluginApi): Row[] {
  const number = (key: "inProgressLimit" | "helperRetries" | "sendBackStopThreshold", label: string, min: number) => ({
    label,
    value: String(draft[key]),
    edit: () =>
      settingsPrompt(api, label, String(draft[key]), (value) => {
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

function agentRows(draft: Draft, setDraft: (draft: Draft) => void, api: TuiPluginApi): Row[] {
  return [
    {
      label: "intakeAgent",
      value: draft.intakeAgent || "session default",
      edit: () =>
        settingsPrompt(api, "intakeAgent", draft.intakeAgent, (value) =>
          setDraft({ ...draft, intakeAgent: value.trim() }),
        ),
    },
    {
      label: "validatorAgent",
      value: draft.validatorAgent || "session default",
      edit: () =>
        settingsPrompt(api, "validatorAgent", draft.validatorAgent, (value) =>
          setDraft({ ...draft, validatorAgent: value.trim() }),
        ),
    },
  ]
}

function commandRows(draft: Draft, setDraft: (draft: Draft) => void, api: TuiPluginApi): Row[] {
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

function validatorModelRows(draft: Draft, setDraft: (draft: Draft) => void, api: TuiPluginApi): Row[] {
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

export function rowsFor(section: Section, draft: Draft, setDraft: (draft: Draft) => void, api: TuiPluginApi): Row[] {
  if (section === "General") return generalRows(draft, setDraft, api)
  if (section === "Agents") return agentRows(draft, setDraft, api)
  if (section === "Commands") return commandRows(draft, setDraft, api)
  if (section === "Validator models") return validatorModelRows(draft, setDraft, api)
  return [{ label: "plugin options", value: pluginOptionsJson(draft) }]
}
