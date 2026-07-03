/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { BorderCharacters, Renderable } from "@opentui/core"
import { TextAttributes } from "@opentui/core"
import type { SessionStatus } from "@opencode-ai/sdk/v2"
import { For, Show, createSignal, onCleanup } from "solid-js"
import {
  type Badge,
  formatAge,
  formatDiff,
  formatModeRationale,
  gateBadges,
  shortSubtaskTitle,
  summarizeSubtasks,
} from "./format"
import { intakeReady, kagan } from "./task"
import type { BoardSession } from "./types"

const LEFT_BORDER_CHARS: BorderCharacters = {
  topLeft: "",
  topRight: "",
  bottomLeft: "",
  bottomRight: "",
  horizontal: " ",
  vertical: "┃",
  topT: "",
  bottomT: "",
  leftT: "",
  rightT: "",
  cross: "",
}

function isReady(session: BoardSession): boolean {
  return (
    session.kaganStatus === "backlog" && kagan(session.metadata).boardTask === true && intakeReady(session.metadata)
  )
}

function taskLabel(session: BoardSession): string {
  const title = session.title || session.slug
  const number = kagan(session.metadata).taskNumber
  return number !== undefined ? `#${number} ${title}` : title
}

function detailSegments(
  session: BoardSession,
  selected: boolean,
  now: number,
  sendBackStopThreshold?: number,
): Badge[] {
  const segments: Badge[] = [{ text: formatAge(session.time.updated, now), tone: "muted" }]
  if (selected) {
    const diff = formatDiff(session.summary)
    if (diff) segments.push({ text: diff, tone: "muted" })
  }
  segments.push(...gateBadges(session.metadata, sendBackStopThreshold))
  return segments
}

function DetailLine(props: {
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

function workingBadge(
  status: SessionStatus["type"] | undefined,
): { text: string; tone: "success" | "warning" } | undefined {
  if (status === "busy") return { text: " · ● working", tone: "success" }
  if (status === "retry") return { text: " · ↻ retrying", tone: "warning" }
  return undefined
}

function SubtaskLine(props: {
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

export function Card(props: {
  api: TuiPluginApi
  session: BoardSession
  children?: BoardSession[]
  selectedID?: string
  sendBackStopThreshold?: number
  checkCommand?: string
  sessionStatus?: (id: string) => SessionStatus["type"] | undefined
  onSelect: (id: string) => void
  onCardRef?: (id: string, node: Renderable | undefined) => void
}) {
  const theme = () => props.api.theme.current
  const [renderedAt] = createSignal(Date.now())
  onCleanup(() => props.onCardRef?.(props.session.id, undefined))
  const children = () => props.children ?? []
  const parentSelected = () => props.selectedID === props.session.id
  const familyFocused = () => parentSelected() || children().some((child) => child.id === props.selectedID)
  const expanded = () => familyFocused() && children().length > 0
  const modeText = () => formatModeRationale(props.session.metadata, props.checkCommand)
  const working = () =>
    props.session.kaganStatus === "in_progress" ? workingBadge(props.sessionStatus?.(props.session.id)) : undefined
  const barColor = () =>
    familyFocused()
      ? theme().primary
      : kagan(props.session.metadata).awaitingInput
        ? theme().warning
        : isReady(props.session)
          ? theme().success
          : theme().border

  return (
    <box
      ref={(node) => props.onCardRef?.(props.session.id, node)}
      flexDirection="column"
      border={["left"]}
      customBorderChars={LEFT_BORDER_CHARS}
      borderColor={barColor()}
    >
      <box flexDirection="column" onMouseDown={() => props.onSelect(props.session.id)}>
        <box flexDirection="row" paddingLeft={1} backgroundColor={parentSelected() ? theme().primary : undefined}>
          <text
            flexGrow={1}
            flexShrink={1}
            wrapMode="none"
            truncate={true}
            fg={parentSelected() ? theme().selectedListItemText : theme().text}
            attributes={parentSelected() ? TextAttributes.BOLD : undefined}
          >
            {taskLabel(props.session)}
          </text>
        </box>
        <box flexDirection="row">
          <DetailLine
            api={props.api}
            session={props.session}
            selected={parentSelected()}
            now={renderedAt()}
            sendBackStopThreshold={props.sendBackStopThreshold}
          />
          <Show when={working()}>
            {(badge) => (
              <text flexShrink={0} wrapMode="none" fg={theme()[badge().tone]}>
                {badge().text}
              </text>
            )}
          </Show>
        </box>
        <Show when={parentSelected() && modeText()}>
          <text paddingLeft={1} flexShrink={1} wrapMode="word" fg={theme().textMuted}>
            {modeText()}
          </text>
        </Show>
      </box>
      <Show when={!expanded() && children().length > 0}>
        <text paddingLeft={1} flexShrink={1} wrapMode="none" truncate={true} fg={theme().textMuted}>
          {summarizeSubtasks(children())}
        </text>
      </Show>
      <Show when={expanded()}>
        <For each={children()}>
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
    </box>
  )
}
