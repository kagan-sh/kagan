/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { TextAttributes } from "@opentui/core"
import { type Accessor, For, Show } from "solid-js"
import { boardBindings } from "../commands.tsx"

export function HelpOverlay(props: { api: TuiPluginApi; visible: () => boolean; updateAvailable: Accessor<boolean> }) {
  const theme = () => props.api.theme.current
  const bindings = () => boardBindings(props.updateAvailable()).filter((binding) => binding.cmd !== "kagan.close")

  return (
    <Show when={props.visible()}>
      <box
        position="absolute"
        top={3}
        left={0}
        bottom={2}
        right={0}
        padding={1}
        zIndex={1}
        backgroundColor={theme().backgroundPanel}
      >
        <box flexDirection="row" justifyContent="space-between" flexShrink={0}>
          <text fg={theme().text} attributes={TextAttributes.BOLD}>
            Help
          </text>
          <text fg={theme().textMuted}>esc</text>
        </box>
        <text flexShrink={0} fg={theme().textMuted}>
          New here? Run /kagan-tutorial for the tour.
        </text>
        <scrollbox flexGrow={1} scrollY={true} verticalScrollbarOptions={{ visible: false }}>
          <box flexDirection="column">
            <For each={bindings()}>
              {(binding) => (
                <box flexDirection="row" gap={2}>
                  <text flexShrink={0} wrapMode="none" fg={theme().textMuted}>
                    {binding.key}
                  </text>
                  <text flexGrow={1} wrapMode="none" truncate={true} fg={theme().text}>
                    {binding.desc}
                  </text>
                </box>
              )}
            </For>
          </box>
        </scrollbox>
      </box>
    </Show>
  )
}
