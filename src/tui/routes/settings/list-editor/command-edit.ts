import { commandSpec } from "../../../../domain/task/commands"
import type { CommandSpec } from "../../../../domain/task/types"
import { appendItem } from "./state"
import type { EditorContext } from "./types"

const COMMAND_FIELDS = ["name", "cwd", "command", "scope"] as const

export type CommandField = (typeof COMMAND_FIELDS)[number]

export { COMMAND_FIELDS }

function parseScope(text?: string): string[] | undefined {
  const trimmed = text?.trim()
  return trimmed
    ? trimmed
        .split(",")
        .map((pattern) => pattern.trim())
        .filter(Boolean)
    : undefined
}

function finalizeCommand(
  ctx: EditorContext<CommandSpec>,
  values: Partial<Record<CommandField, string>>,
  fallbackName: string,
) {
  const name = values.name?.trim()
  const cwd = values.cwd?.trim()
  const command = values.command?.trim()
  if (!name || !command || cwd === undefined) {
    ctx.setMessage("Name, cwd, and command are required")
    ctx.reopenWithSnapshot()
    return
  }
  if (!cwd) {
    ctx.setMessage("cwd cannot be empty")
    ctx.reopenWithSnapshot()
    return
  }
  const parsed = commandSpec({ name, cwd, command, scope: parseScope(values.scope) }, fallbackName)
  if (!parsed) {
    ctx.setMessage("Invalid command: unsafe cwd or invalid scope regex")
    ctx.reopenWithSnapshot()
    return
  }
  appendItem(ctx, parsed)
}

export function addCommand(ctx: EditorContext<CommandSpec>, kind: "setup" | "check") {
  const values: Partial<Record<CommandField, string>> = {}

  const ask = (fields: CommandField[]) => {
    if (fields.length === 0) {
      finalizeCommand(ctx, values, `${kind} ${ctx.items().length + 1}`)
      return
    }

    const [field, ...rest] = fields
    if (!field) return
    const title = field === "scope" ? "scope (comma-separated regexes)" : field
    ctx.prompt(title, values[field] ?? "", (next) => {
      values[field] = next
      ask(rest)
    })
  }

  ask(["name", "cwd", "command", "scope"])
}

export function editCommandField(ctx: EditorContext<CommandSpec>) {
  const command = ctx.items()[ctx.selectedRow()]
  if (!command) return
  const field = ctx.focusedField() as CommandField
  const value = field === "scope" ? (command.scope ?? []).join(", ") : command[field]
  const title = field === "scope" ? "scope (comma-separated regexes)" : field
  ctx.prompt(title, value, (next) => {
    const updated = { ...command }
    if (field === "scope") {
      updated.scope = next
        ? next
            .split(",")
            .map((pattern) => pattern.trim())
            .filter(Boolean)
        : undefined
    } else {
      updated[field] = next.trim()
    }
    if (!updated.name || !updated.command || !updated.cwd) {
      ctx.setMessage("Name, cwd, and command are required")
      ctx.reopenWithSnapshot()
      return
    }
    const parsed = commandSpec(updated, command.name)
    if (!parsed) {
      ctx.setMessage("Invalid command: unsafe cwd or invalid scope regex")
      ctx.reopenWithSnapshot()
      return
    }
    const nextCommands = [...ctx.items()]
    nextCommands[ctx.selectedRow()] = parsed
    ctx.setItems(nextCommands)
    ctx.setMessage(undefined)
    ctx.reopenWithSnapshot()
  })
}
