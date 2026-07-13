/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Renderable } from "@opentui/core"
import { For, Show } from "solid-js"
import { TextAttributes } from "@opentui/core"
import { formatModeRationale, summarizeSubtasks } from "../../format"
import type { BoardSession } from "../../types"
import { kagan } from "../../../domain/task/metadata"
import { isReady, taskLabel, workingBadge } from "./helpers"
import { DetailLine } from "./detail"
import { SubtaskLine } from "./subtask"
import { SIDE_BORDER_CHARS } from "../borders"
import type { CardDisplayProps } from "./shell-props"

function CardHeader(props: {
  api: TuiPluginApi
  session: BoardSession
  parentSelected: boolean
  renderedAt: number
  sendBackStopThreshold?: number
  checkCommand?: string
  working?: { text: string; tone: "success" | "warning" }
  onSelect: () => void
}) {
  const theme = () => props.api.theme.current
  const modeText = () => formatModeRationale(props.session.metadata, props.checkCommand)

  return (
    <box flexDirection="column" onMouseDown={props.onSelect}>
      <box flexDirection="row" paddingLeft={1} backgroundColor={props.parentSelected ? theme().primary : undefined}>
        <text
          flexGrow={1}
          flexShrink={1}
          wrapMode="none"
          truncate={true}
          fg={props.parentSelected ? theme().selectedListItemText : theme().text}
          attributes={props.parentSelected ? TextAttributes.BOLD : undefined}
        >
          {taskLabel(props.session)}
        </text>
      </box>
      <box flexDirection="row">
        <DetailLine
          api={props.api}
          session={props.session}
          selected={props.parentSelected}
          now={props.renderedAt}
          sendBackStopThreshold={props.sendBackStopThreshold}
        />
        <Show when={props.working}>
          {(badge) => (
            <text flexShrink={0} wrapMode="none" fg={theme()[badge().tone]}>
              {badge().text}
            </text>
          )}
        </Show>
      </box>
      <Show when={props.parentSelected && modeText()}>
        <text paddingLeft={1} flexShrink={1} wrapMode="word" fg={theme().textMuted}>
          {modeText()}
        </text>
      </Show>
    </box>
  )
}

function CardChildren(props: {
  api: TuiPluginApi
  children: BoardSession[]
  expanded: boolean
  selectedID?: string
  onSelect: (id: string) => void
  onCardRef?: (id: string, node: Renderable | undefined) => void
}) {
  const theme = () => props.api.theme.current

  return (
    <>
      <Show when={!props.expanded && props.children.length > 0}>
        <text paddingLeft={1} flexShrink={1} wrapMode="none" truncate={true} fg={theme().textMuted}>
          {summarizeSubtasks(props.children)}
        </text>
      </Show>
      <Show when={props.expanded}>
        <For each={props.children}>
          {(child) => (
            <SubtaskLine
              api={props.api}
              session={child}
              selected={child.id === props.selectedID}
              onMouseDown={() => props.onSelect(child.id)}
              onRef={(node) => props.onCardRef?.(child.id, node)}
            />
          )}
        </For>
      </Show>
    </>
  )
}

export function CardShell(props: CardDisplayProps & { renderedAt: number }) {
  const theme = () => props.api.theme.current
  const childSessions = () => props.children ?? []
  const parentSelected = () => props.selectedID === props.session.id
  const familyFocused = () => parentSelected() || childSessions().some((child) => child.id === props.selectedID)
  const expanded = () => familyFocused() && childSessions().length > 0
  const working = () =>
    props.session.kaganStatus === "in_progress" ? workingBadge(props.sessionStatus?.(props.session.id)) : undefined
  const barColor = () => {
    if (familyFocused()) return theme().primary
    if ((kagan(props.session.metadata).awaitingPermissions?.length ?? 0) > 0) return theme().warning
    if (isReady(props.session)) return theme().success
    return theme().border
  }

  return (
    <box
      ref={(node) => props.onCardRef?.(props.session.id, node)}
      flexDirection="column"
      border={["left"]}
      customBorderChars={SIDE_BORDER_CHARS}
      borderColor={barColor()}
    >
      <CardHeader
        api={props.api}
        session={props.session}
        parentSelected={parentSelected()}
        renderedAt={props.renderedAt}
        sendBackStopThreshold={props.sendBackStopThreshold}
        checkCommand={props.checkCommand}
        working={working()}
        onSelect={() => props.onSelect(props.session.id)}
      />
      <CardChildren
        api={props.api}
        children={childSessions()}
        expanded={expanded()}
        selectedID={props.selectedID}
        onSelect={props.onSelect}
        onCardRef={props.onCardRef}
      />
    </box>
  )
}
