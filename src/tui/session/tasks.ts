import type { TuiPluginApi } from "@opencode-ai/plugin/tui"
import type { Session } from "@opencode-ai/sdk/v2"
import { approveDenyReason, helper, helperRestartPatch } from "../../domain/task/policy"
import { kagan } from "../../domain/task/metadata"
import { resolveFinding, type FindingResolution } from "../../domain/task/findings"
import { resolveIntakeDecision } from "../../domain/task/intake"
import type { ColumnType, HelperRole } from "../../domain/task/types"
import { tuiPatchKagan } from "./patch"

export async function listSessions(api: TuiPluginApi): Promise<Session[]> {
  const result = await api.client.session.list({ scope: "project" }, { throwOnError: true })
  return result.data ?? []
}

export async function moveSession(api: TuiPluginApi, sessionID: string, status: ColumnType): Promise<void> {
  await tuiPatchKagan(api, sessionID, { status })
}

export async function resolveSessionFinding(
  api: TuiPluginApi,
  sessionID: string,
  session: Session,
  findingID: string,
  resolution: FindingResolution,
  note?: string,
): Promise<void> {
  const findings = kagan(session.metadata).findings ?? []
  await tuiPatchKagan(api, sessionID, { findings: resolveFinding(findings, findingID, resolution, note) })
}

export async function resolveSessionIntakeDecision(
  api: TuiPluginApi,
  sessionID: string,
  session: Session,
  decisionID: string,
  resolution: "approved" | "overridden",
  answer?: string,
): Promise<void> {
  const intake = kagan(session.metadata).intake
  if (!intake) return
  const decisions = resolveIntakeDecision(intake.decisions, decisionID, resolution, answer)
  await tuiPatchKagan(api, sessionID, { intake: { ...intake, decisions } })
}

async function abortSession(api: TuiPluginApi, sessionID: string): Promise<void> {
  try {
    await api.client.session.abort({ sessionID }, { throwOnError: true })
  } catch {
    // already idle or gone
  }
}

export async function retryHelper(
  api: TuiPluginApi,
  sessionID: string,
  session: Session,
  status: ColumnType,
): Promise<void> {
  if (status !== "backlog" && status !== "review") throw new Error("Restart only applies to backlog or review tasks")
  const role: HelperRole = status === "backlog" ? "intake" : "validator"
  const liveID = helper(session.metadata, role).sessionID
  await tuiPatchKagan(api, sessionID, helperRestartPatch(role))
  if (liveID) await abortSession(api, liveID)
}

export async function approveSession(api: TuiPluginApi, sessionID: string, session: Session): Promise<void> {
  const reason = approveDenyReason(session.metadata)
  if (reason) throw new Error(reason)
  await tuiPatchKagan(api, sessionID, { approved: true })
}

export async function archiveSession(api: TuiPluginApi, sessionID: string): Promise<void> {
  await api.client.session.update({ sessionID, time: { archived: Date.now() } }, { throwOnError: true })
}
