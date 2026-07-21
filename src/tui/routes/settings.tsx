/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { For, Show, createMemo, createSignal } from "solid-js"
import { draftFromOptions, pluginOptionsJson, SECTIONS } from "./settings/draft"
import { rowsFor } from "./settings/rows"
import { useSettingsKeys } from "./settings/keys"

export function Settings(props: { api: TuiPluginApi; options?: Record<string, unknown> }) {
  const theme = () => props.api.theme.current
  const [draft, setDraft] = createSignal(draftFromOptions(props.options))
  const [sectionIndex, setSectionIndex] = createSignal(0)
  const [rowIndex, setRowIndex] = createSignal(0)
  const [message, setMessage] = createSignal<string>()
  const section = () => SECTIONS[sectionIndex()] ?? "General"
  const rows = createMemo(() => rowsFor(section(), draft(), setDraft, props.api))

  useSettingsKeys({
    api: props.api,
    draft,
    rows,
    sectionIndex,
    setSectionIndex,
    rowIndex,
    setRowIndex,
    setMessage,
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
