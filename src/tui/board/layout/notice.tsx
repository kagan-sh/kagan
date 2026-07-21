/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi, TuiThemeCurrent, TuiToast } from "@opencode-ai/plugin/tui"
import { For } from "solid-js"
import { useRendererDimensions } from "../../renderer"
import { SIDE_BORDER_CHARS } from "../borders"
import type { BoardStore } from "../store"

function noticeBorderColor(theme: TuiThemeCurrent, variant?: TuiToast["variant"]) {
  if (variant === "error") return theme.error
  if (variant === "warning") return theme.warning
  if (variant === "success") return theme.success
  return theme.info
}

// api.ui.toast doesn't render on plugin routes — see the Notice rationale in store-notices.tsx.
export function Notice(props: { api: TuiPluginApi; store: BoardStore }) {
  const theme = () => props.api.theme.current
  const dimensions = useRendererDimensions(props.api)

  return (
    <box position="absolute" top={2} right={2} flexDirection="column" alignItems="flex-end" gap={1} zIndex={2}>
      <For each={props.store.notices()}>
        {(notice) => (
          <box
            maxWidth={Math.min(60, dimensions().width - 6)}
            paddingLeft={2}
            paddingRight={2}
            paddingTop={1}
            paddingBottom={1}
            border={["left", "right"]}
            customBorderChars={SIDE_BORDER_CHARS}
            borderColor={noticeBorderColor(theme(), notice.variant)}
            backgroundColor={theme().backgroundPanel}
          >
            <text fg={theme().text} wrapMode="word">
              {notice.message}
            </text>
          </box>
        )}
      </For>
    </box>
  )
}
