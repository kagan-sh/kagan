import { clean, gt } from "semver"

const REGISTRY_DIST_TAGS = "https://registry.npmjs.org/-/package/@kagan-sh/kagan/dist-tags"
const CHECK_TTL_MS = 60 * 60 * 1000
const FETCH_TIMEOUT_MS = 3000
const LAST_CHECK_KEY = "kagan:update:lastCheck"
const LATEST_KEY = "kagan:update:latest"
export const KAGAN_PACKAGE = "@kagan-sh/kagan"

export type UpdateStatus =
  | { kind: "available"; version: string }
  | { kind: "installing"; version: string }
  | { kind: "restart"; version: string }
  | undefined

export type UpdateCheck =
  | Extract<UpdateStatus, { kind: "available" }>
  | { kind: "current" }
  | { kind: "ineligible" }
  | undefined

type UpdateKv = {
  get: <Value = unknown>(key: string, fallback?: Value) => Value
  set: (key: string, value: unknown) => void
}

export function parseRelease(version: string): string | undefined {
  const normalized = clean(version.trim())
  return normalized === version.trim() && /^\d+\.\d+\.\d+$/.test(normalized) ? normalized : undefined
}

function supportedSpec(spec: string): boolean {
  if (spec === KAGAN_PACKAGE || spec === `${KAGAN_PACKAGE}@latest`) return true
  if (!spec.startsWith(`${KAGAN_PACKAGE}@`)) return false
  return parseRelease(spec.slice(KAGAN_PACKAGE.length + 1)) !== undefined
}

export function isUpdateEligibleInstall(input: { source: string; spec: string; version: string }): boolean {
  return input.source === "npm" && supportedSpec(input.spec) && parseRelease(input.version) !== undefined
}

export function isNewerRelease(latest: string, current: string): boolean {
  const next = parseRelease(latest)
  const now = parseRelease(current)
  return Boolean(next && now && gt(next, now))
}

async function fetchLatest(fetchImpl: typeof fetch): Promise<string | undefined> {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), FETCH_TIMEOUT_MS)
  try {
    const response = await fetchImpl(REGISTRY_DIST_TAGS, { signal: controller.signal })
    if (!response.ok) return
    const tags = (await response.json()) as { latest?: unknown }
    return typeof tags.latest === "string" ? parseRelease(tags.latest) : undefined
  } catch {
    return
  } finally {
    clearTimeout(timer)
  }
}

export async function resolveLatestRelease(
  kv: UpdateKv,
  now: number,
  deps: { fetchImpl?: typeof fetch; force?: boolean } = {},
): Promise<string | undefined> {
  const cached = parseRelease(kv.get<string>(LATEST_KEY, ""))
  const lastCheck = kv.get<number>(LAST_CHECK_KEY, 0)
  if (cached && !deps.force && lastCheck <= now && now - lastCheck < CHECK_TTL_MS) return cached

  const latest = await fetchLatest(deps.fetchImpl ?? fetch)
  if (!latest) return
  kv.set(LATEST_KEY, latest)
  kv.set(LAST_CHECK_KEY, now)
  return latest
}

export type CheckInput = {
  kv: UpdateKv
  currentVersion: string
  source: "file" | "npm" | "internal"
  spec: string
  now: number
  force?: boolean
  fetchImpl?: typeof fetch
}

export async function checkForUpdate(input: CheckInput): Promise<UpdateCheck> {
  if (!isUpdateEligibleInstall({ source: input.source, spec: input.spec, version: input.currentVersion }))
    return { kind: "ineligible" }
  const latest = await resolveLatestRelease(input.kv, input.now, {
    fetchImpl: input.fetchImpl,
    force: input.force,
  })
  if (!latest) return
  if (isNewerRelease(latest, input.currentVersion)) return { kind: "available", version: latest }
  return { kind: "current" }
}
