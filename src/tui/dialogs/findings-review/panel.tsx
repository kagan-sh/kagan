/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import { createSignal, onMount, Show } from "solid-js"
import type { FindingResolution } from "../../../domain/task/findings"
import { approveDenyReason } from "../../../domain/task/policy"
import { kagan } from "../../../domain/task/metadata"
import { formatModeRationale } from "../../format"
import { DialogFrame } from "../chrome"
import { FindingDetail, FindingsFooter, FindingsList } from "./views"
import type { createBoardStore } from "../../board/store"
import type { BoardSession } from "../../types"
import { useKeyIntercept } from "../../renderer"
import { commitFindingResolution, handleFindingsDetailKey, handleFindingsListKey, sortedFindings } from "./actions"

type BoardStore = ReturnType<typeof createBoardStore>
type Mode = "list" | "detail"

export function FindingsReview(props: {
  api: TuiPluginApi
  store: BoardStore
  session: BoardSession
  checkCommand?: string
  onApprove: (session: BoardSession) => void
  onSendBack: () => void
}) {
  const theme = () => props.api.theme.current
  const [session, setSession] = createSignal(props.session)
  const [mode, setMode] = createSignal<Mode>("list")
  const [index, setIndex] = createSignal(0)
  const [focus, setFocus] = createSignal(0)
  const [note, setNote] = createSignal("")
  const [error, setError] = createSignal<string | undefined>()
  const modeText = () => formatModeRationale(session().metadata, props.checkCommand)

  onMount(() => props.api.ui.dialog.setSize("large"))

  const findings = () => sortedFindings(session().metadata ?? {})
  const clean = () => findings().length === 0
  const current = () => findings()[index()]
  const reason = () => approveDenyReason(session().metadata)
  const close = () => props.api.ui.dialog.clear()
  const runSendBack = () => {
    close()
    props.onSendBack()
  }
  const runApprove = () => {
    if (reason()) return
    close()
    props.onApprove(session())
  }
  const openDetail = (i: number) => {
    const finding = findings()[i]
    if (!finding) return
    setIndex(i)
    setNote(finding.note ?? "")
    setError(undefined)
    setFocus(0)
    setMode("detail")
  }
  const commit = (resolution: FindingResolution) => {
    const finding = current()
    if (!finding) return
    void commitFindingResolution({
      api: props.api,
      store: props.store,
      sessionId: session().id,
      session: session(),
      finding,
      resolution,
      note: note(),
      setSession,
      setMode,
      setError,
    })
  }

  useKeyIntercept(props.api, (key) =>
    mode() === "list"
      ? handleFindingsListKey({
          key,
          findings: findings(),
          index,
          setIndex,
          close,
          runSendBack,
          runApprove,
          openDetail,
        })
      : handleFindingsDetailKey({ key, focus, setFocus, setError, setMode, commit }),
  )

  const title = () => {
    const number = kagan(session().metadata).taskNumber
    return number !== undefined ? `Approve #${number} — triage findings first` : "Approve — triage findings first"
  }

  return (
    <DialogFrame api={props.api} title={title()}>
      <Show when={modeText()}>
        <text fg={theme().textMuted} wrapMode="word">
          {modeText()}
        </text>
      </Show>
      <Show when={mode() === "list"}>
        <Show when={!clean()} fallback={<text fg={theme().textMuted}>No findings — review is clean.</text>}>
          <FindingsList theme={theme()} findings={findings()} index={index()} />
        </Show>
      </Show>
      <Show when={mode() === "detail" && current()}>
        {(finding) => (
          <FindingDetail
            theme={theme()}
            finding={finding()}
            index={index()}
            total={findings().length}
            note={note()}
            setNote={setNote}
            focus={focus()}
            error={error()}
          />
        )}
      </Show>
      <FindingsFooter theme={theme()} mode={mode()} clean={clean()} reason={reason()} />
    </DialogFrame>
  )
}
