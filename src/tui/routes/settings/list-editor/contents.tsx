/** @jsxImportSource @opentui/solid */
import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import type { Accessor } from "solid-js"
import { For, Show } from "solid-js"
import type { ListEditorColumn, ListEditorContentsProps } from "./state"

function fieldColor(theme: TuiThemeCurrent, selected: boolean, focused: boolean) {
  return selected && focused ? theme.text : selected ? theme.selectedListItemText : theme.text
}

function ListEditorRow<T>(props: {
  item: T
  index: Accessor<number>
  selectedRow: Accessor<number>
  focusedField: Accessor<string>
  columns: ListEditorColumn<T>[]
  theme: Accessor<TuiThemeCurrent>
}) {
  const selected = () => props.index() === props.selectedRow()
  return (
    <box flexDirection="row" gap={1} backgroundColor={selected() ? props.theme().primary : undefined}>
      <For each={props.columns}>
        {(column) => (
          <text
            width={column.width}
            flexGrow={column.flexGrow}
            wrapMode="none"
            fg={fieldColor(props.theme(), selected(), props.focusedField() === column.field)}
          >
            {column.value(props.item)}
          </text>
        )}
      </For>
    </box>
  )
}

export function ListEditorContents<T>(props: ListEditorContentsProps<T>) {
  const theme = props.theme
  return (
    <>
      <box flexDirection="column" gap={1}>
        <Show when={props.items().length > 0} fallback={<text fg={theme().textMuted}>{props.empty}</text>}>
          <For each={props.items()}>
            {(item, index) => (
              <ListEditorRow
                item={item}
                index={index}
                selectedRow={props.selectedRow}
                focusedField={props.focusedField}
                columns={props.columns}
                theme={theme}
              />
            )}
          </For>
        </Show>
      </box>
      <Show when={props.message()}>
        <text fg={theme().error}>{props.message()}</text>
      </Show>
      <box paddingTop={1} flexDirection="row" gap={2}>
        <text fg={theme().text}>
          enter <span style={{ fg: theme().textMuted }}>edit</span>
        </text>
        <text fg={theme().text}>
          a <span style={{ fg: theme().textMuted }}>add</span>
        </text>
        <text fg={theme().text}>
          d <span style={{ fg: theme().textMuted }}>delete</span>
        </text>
        <Show when={props.reorder}>
          <text fg={theme().text}>
            shift+↑↓ <span style={{ fg: theme().textMuted }}>reorder</span>
          </text>
        </Show>
      </box>
    </>
  )
}
