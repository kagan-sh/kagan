/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { Show } from "solid-js"
import { formatModeRationale } from "../../format"
import { DialogFrame } from "../chrome"
import type { BoardStore } from "../../board/store"
import type { BoardSession } from "../../types"
import { FindingDetail, FindingsFooter, FindingsList } from "./views"
import { useFindingsReviewState } from "./use-review"

function FindingsReview(props: {
  api: TuiPluginApi
  store: BoardStore
  session: BoardSession
  checkCommand?: string
  onApprove: (session: BoardSession) => void
  onSendBack: () => void
}) {
  const theme = () => props.api.theme.current
  const state = useFindingsReviewState(props)
  const modeText = () => formatModeRationale(props.session.metadata, props.checkCommand)

  return (
    <DialogFrame api={props.api} title={state.title()}>
      <Show when={modeText()}>
        <text fg={theme().textMuted} wrapMode="word">
          {modeText()}
        </text>
      </Show>
      <Show when={state.mode() === "list"}>
        <Show when={!state.clean()} fallback={<text fg={theme().textMuted}>No findings — review is clean.</text>}>
          <FindingsList theme={theme()} findings={state.findings()} index={state.index()} />
        </Show>
      </Show>
      <Show when={state.mode() === "detail" && state.current()}>
        {(finding) => (
          <FindingDetail
            theme={theme()}
            finding={finding()}
            index={state.index()}
            total={state.findings().length}
            note={state.note()}
            setNote={state.setNote}
            focus={state.focus()}
            error={state.error()}
          />
        )}
      </Show>
      <FindingsFooter theme={theme()} mode={state.mode()} clean={state.clean()} reason={state.reason()} />
    </DialogFrame>
  )
}

export function openFindingsReviewDialog(
  api: TuiPluginApi,
  store: BoardStore,
  session: BoardSession,
  checkCommand: string | undefined,
  callbacks: { onApprove: (session: BoardSession) => void; onSendBack: () => void },
): void {
  api.ui.dialog.replace(() => (
    <FindingsReview api={api} store={store} session={session} checkCommand={checkCommand} {...callbacks} />
  ))
}
