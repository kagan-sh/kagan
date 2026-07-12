import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Renderable } from "@opentui/core"
import type { SessionStatus } from "@opencode-ai/sdk/v2"
import type { BoardSession } from "../../types"

export type CardDisplayProps = {
  api: TuiPluginApi
  session: BoardSession
  children?: BoardSession[]
  selectedID?: string
  sendBackStopThreshold?: number
  checkCommand?: string
  sessionStatus?: (id: string) => SessionStatus["type"] | undefined
  onSelect: (id: string) => void
  onCardRef?: (id: string, node: Renderable | undefined) => void
}
