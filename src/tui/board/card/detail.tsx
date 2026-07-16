/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { For, Show } from "solid-js"
import type { Badge } from "../../format"
import type { BoardSession } from "../../types"
import { detailSegments } from "./helpers"

export function DetailLine(props: {
  api: TuiPluginApi
  session: BoardSession
  selected: boolean
  now: number
  sendBackStopThreshold?: number
}) {
  const theme = () => props.api.theme.current
  const segments = () => detailSegments(props.session, props.selected, props.now, props.sendBackStopThreshold)
  const toneColor = (tone: Badge["tone"]) => (tone === "muted" ? theme().textMuted : theme()[tone])

  return (
    <text paddingLeft={1} flexShrink={1} wrapMode="none" truncate={true}>
      <For each={segments()}>
        {(segment, index) => (
          <>
            <Show when={index() > 0}>
              <span style={{ fg: theme().textMuted }}> · </span>
            </Show>
            <span style={{ fg: toneColor(segment.tone) }}>{segment.text}</span>
          </>
        )}
      </For>
    </text>
  )
}
