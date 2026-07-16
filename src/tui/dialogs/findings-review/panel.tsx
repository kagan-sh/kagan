/** @jsxImportSource @opentui/solid */
import { Show } from "solid-js"
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { formatModeRationale } from "../../format"
import { DialogFrame } from "../chrome"
import { FindingDetail, FindingsFooter, FindingsList } from "./views"
import type { createBoardStore } from "../../board/store"
import type { BoardSession } from "../../types"
import { useFindingsReviewState } from "./use-review"

type BoardStore = ReturnType<typeof createBoardStore>

export function FindingsReview(props: {
  api: TuiPluginApi
  store: BoardStore
  session: BoardSession
  checkCommand?: string
  onApprove: (session: BoardSession) => void
  onSendBack: () => void
}) {
  const theme = () => props.api.theme.current
  const review = useFindingsReviewState(props)
  const modeText = () => formatModeRationale(review.session().metadata, props.checkCommand)

  return (
    <DialogFrame api={props.api} title={review.title()}>
      <Show when={modeText()}>
        <text fg={theme().textMuted} wrapMode="word">
          {modeText()}
        </text>
      </Show>
      <Show when={review.mode() === "list"}>
        <Show when={!review.clean()} fallback={<text fg={theme().textMuted}>No findings — review is clean.</text>}>
          <FindingsList theme={theme()} findings={review.findings()} index={review.index()} />
        </Show>
      </Show>
      <Show when={review.mode() === "detail" && review.current()}>
        {(finding) => (
          <FindingDetail
            theme={theme()}
            finding={finding()}
            index={review.index()}
            total={review.findings().length}
            note={review.note()}
            setNote={review.setNote}
            focus={review.focus()}
            error={review.error()}
          />
        )}
      </Show>
      <FindingsFooter theme={theme()} mode={review.mode()} clean={review.clean()} reason={review.reason()} />
    </DialogFrame>
  )
}
