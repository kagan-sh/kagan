/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createEffect, createMemo, createSignal, For, onMount } from "solid-js"
import { useKeyIntercept } from "../renderer"
import { DialogFilter, DialogFrame } from "./chrome"

export type FilterableSelectProps = {
  title: string
  filterPlaceholder: string
  labels: string[]
  selectedIndex: number
  filter: string
  onFilter: (value: string) => void
  onSelect: (index: number) => void
  reopen: () => void
}

function FilterableSelectPicker(props: { api: TuiPluginApi } & FilterableSelectProps) {
  const theme = () => props.api.theme.current
  const [filter, setFilter] = createSignal(props.filter)
  const [listIndex, setListIndex] = createSignal(0)
  const options = createMemo(() => {
    const query = filter().trim().toLowerCase()
    return props.labels
      .map((label, index) => ({ label, index }))
      .filter(({ label }) => !query || label.toLowerCase().includes(query))
  })
  createEffect(() => {
    const last = Math.max(0, options().length - 1)
    setListIndex((value) => Math.min(value, last))
  })
  const close = (index?: number) => {
    props.onFilter(filter())
    if (index !== undefined) props.onSelect(index)
    props.reopen()
  }

  onMount(() => {
    props.api.ui.dialog.setSize("medium")
    const selected = options().findIndex((option) => option.index === props.selectedIndex)
    if (selected >= 0) setListIndex(selected)
  })

  useKeyIntercept(props.api, (key) => {
    if (key.name === "escape") {
      close()
      return true
    }
    if (key.name === "down") {
      setListIndex((value) => Math.min(value + 1, Math.max(0, options().length - 1)))
      return true
    }
    if (key.name === "up") {
      setListIndex((value) => Math.max(value - 1, 0))
      return true
    }
    if (key.name === "return") {
      const option = options()[listIndex()]
      if (option) close(option.index)
      else close()
      return true
    }
    return false
  })

  return (
    <DialogFrame api={props.api} title={props.title}>
      <DialogFilter api={props.api} value={filter()} onInput={setFilter} placeholder={props.filterPlaceholder} />
      <box flexDirection="column">
        <For each={options()}>
          {(option, i) => (
            <box backgroundColor={i() === listIndex() ? theme().primary : undefined}>
              <text fg={i() === listIndex() ? theme().selectedListItemText : theme().text}>{option.label}</text>
            </box>
          )}
        </For>
      </box>
      <box paddingBottom={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>select</span>
        </text>
      </box>
    </DialogFrame>
  )
}

export function openFilterableSelectPicker(api: TuiPluginApi, props: FilterableSelectProps) {
  api.ui.dialog.replace(() => <FilterableSelectPicker api={api} {...props} />)
}
