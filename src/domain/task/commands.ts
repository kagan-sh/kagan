import type { CommandSpec } from "./types"

export type TaskScope = { values: string[]; custom?: string }

function isSafeCwd(cwd: string): boolean {
  if (cwd === "" || cwd.startsWith("/") || cwd.startsWith("\\")) return false
  if (/^[A-Za-z]:[\\/]/.test(cwd)) return false
  return !cwd.split(/[\\/]+/).includes("..")
}

function validScopePatterns(value: unknown): string[] | undefined {
  if (value === undefined) return undefined
  if (!Array.isArray(value)) return undefined
  const patterns: string[] = []
  for (const raw of value) {
    if (typeof raw !== "string") return undefined
    const pattern = raw.trim()
    if (!pattern) return undefined
    try {
      new RegExp(pattern)
    } catch {
      return undefined
    }
    patterns.push(pattern)
  }
  return patterns
}

function stringProperty(raw: Record<string, unknown>, property: string): string {
  return typeof raw[property] === "string" ? raw[property].trim() : ""
}

function normalizedCwd(raw: Record<string, unknown>): string | undefined {
  const cwd = stringProperty(raw, "cwd")
  if (cwd.startsWith("/") || cwd.startsWith("\\") || /^[A-Za-z]:[\\/]/.test(cwd)) return undefined
  const normalized = cwd.replace(/\/+$/, "").replace(/^\.\/+/, "") || "."
  return isSafeCwd(normalized) ? normalized : undefined
}

function objectCommandSpec(value: Record<string, unknown>): CommandSpec | undefined {
  const name = stringProperty(value, "name")
  const cwd = normalizedCwd(value)
  const command = stringProperty(value, "command")
  const scope = validScopePatterns(value.scope)
  if (!name || !cwd || !command || (value.scope !== undefined && scope === undefined)) return undefined
  if (!scope || scope.length === 0) return { name, cwd, command }
  return { name, cwd, command, scope }
}

export function commandSpec(value: unknown, fallbackName: string): CommandSpec | undefined {
  if (typeof value === "string") {
    const command = value.trim()
    return command ? { name: fallbackName, cwd: ".", command } : undefined
  }
  if (typeof value !== "object" || value === null) return undefined
  return objectCommandSpec(value as Record<string, unknown>)
}

function commandSpecs(value: unknown, fallbackPrefix: string): CommandSpec[] {
  if (typeof value === "string") {
    const spec = commandSpec(value, fallbackPrefix)
    return spec ? [spec] : []
  }
  if (!Array.isArray(value)) return []
  return value
    .map((item, index) => commandSpec(item, `${fallbackPrefix} ${index + 1}`))
    .filter((spec): spec is CommandSpec => spec !== undefined)
}

export function commandPlan(options: Record<string, unknown> | undefined, kind: "setup" | "check"): CommandSpec[] {
  const commands = options?.commands
  if (typeof commands !== "object" || commands === null) return []
  return commandSpecs((commands as Record<string, unknown>)[kind], kind)
}

export function configuredScopes(options?: Record<string, unknown>): string[] {
  const seen = new Set<string>()
  for (const command of [...commandPlan(options, "setup"), ...commandPlan(options, "check")]) {
    if (command.cwd === "." || seen.has(command.cwd)) continue
    seen.add(command.cwd)
  }
  return [...seen]
}

export function sanitizeTaskScope(value: unknown): TaskScope | undefined {
  if (typeof value !== "object" || value === null) return undefined
  const raw = value as Record<string, unknown>
  const values = Array.isArray(raw.values)
    ? raw.values
        .filter((item): item is string => typeof item === "string" && item.trim().length > 0)
        .map((item) => item.trim())
    : []
  const unique = [...new Set(values)]
  const custom = typeof raw.custom === "string" && raw.custom.trim() ? raw.custom.trim() : undefined
  return unique.length > 0 || custom ? { values: unique, ...(custom ? { custom } : {}) } : undefined
}

export function commandInTaskScope(command: CommandSpec, scope?: TaskScope): boolean {
  if (command.cwd === ".") return true
  if (!scope) return false
  return scope.values.includes(command.cwd) || scope.custom === command.cwd
}

export function commandMatchesChangedFile(command: CommandSpec, changedFiles: readonly string[]): boolean {
  const cwdPrefix = command.cwd === "." ? "" : `${command.cwd}/`
  const scope = command.scope?.map((pattern) => new RegExp(pattern)) ?? []
  return changedFiles.some(
    (file) => file === command.cwd || file.startsWith(cwdPrefix) || scope.some((regex) => regex.test(file)),
  )
}
