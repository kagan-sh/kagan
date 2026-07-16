/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Renderable } from "@opentui/core"
import { onCleanup } from "solid-js"
import { shortSubtaskTitle } from "../../format"
import type { BoardSession } from "../../types"

export function SubtaskLine(props: {
  api: TuiPluginApi
  session: BoardSession
  selected: boolean
  onMouseDown?: () => void
  onRef?: (node: Renderable | undefined) => void
}) {
  const theme = () => props.api.theme.current
  const fg = () => (props.selected ? theme().selectedListItemText : theme().textMuted)

  onCleanup(() => props.onRef?.(undefined))

  return (
    <box
      ref={(node) => props.onRef?.(node)}
      flexDirection="row"
      paddingLeft={2}
      gap={1}
      backgroundColor={props.selected ? theme().primary : undefined}
      onMouseDown={props.onMouseDown}
    >
      <text flexShrink={0} fg={fg()}>
        └
      </text>
      <text flexGrow={1} flexShrink={1} wrapMode="none" truncate={true} fg={fg()}>
        {shortSubtaskTitle(props.session)}
      </text>
    </box>
  )
}
