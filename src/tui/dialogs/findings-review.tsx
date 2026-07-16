/** @jsxImportSource @opentui/solid */
import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { createBoardStore } from "../board/store"
import type { BoardSession } from "../types"
import { FindingsReview } from "./findings-review/panel"

type BoardStore = ReturnType<typeof createBoardStore>

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
