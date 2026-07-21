import type { TuiThemeCurrent } from "@opencode-ai/plugin/tui"
import { RGBA, SyntaxStyle, type ColorInput } from "@opentui/core"

function color(value: unknown, fallback: ColorInput): ColorInput {
  if (value instanceof RGBA) return value
  if (typeof value === "string" && value.length > 0) return value
  return fallback
}

export function syntaxStyleFromTheme(theme: TuiThemeCurrent): SyntaxStyle {
  const text = color(theme.markdownText ?? theme.text, theme.text as ColorInput)
  const heading = color(theme.markdownHeading ?? theme.primary, theme.primary as ColorInput)
  const code = color(theme.markdownCode ?? theme.accent ?? theme.primary, (theme.accent ?? theme.primary) as ColorInput)
  const codeBlock = color(theme.markdownCodeBlock ?? theme.markdownCode ?? theme.text, text)
  const quote = color(theme.markdownBlockQuote ?? theme.textMuted, theme.textMuted as ColorInput)
  const link = color(theme.markdownLink ?? theme.info ?? theme.primary, (theme.info ?? theme.primary) as ColorInput)
  const list = color(theme.markdownListItem ?? theme.text, text)
  const emph = color(theme.markdownEmph ?? theme.text, text)
  const strong = color(theme.markdownStrong ?? theme.text, text)
  return SyntaxStyle.fromStyles({
    default: { fg: text },
    "markup.heading": { fg: heading, bold: true },
    "markup.heading.1": { fg: heading, bold: true },
    "markup.heading.2": { fg: heading, bold: true },
    "markup.bold": { fg: strong, bold: true },
    "markup.strong": { fg: strong, bold: true },
    "markup.italic": { fg: emph, italic: true },
    "markup.list": { fg: list },
    "markup.quote": { fg: quote, italic: true },
    "markup.raw": { fg: code },
    "markup.raw.block": { fg: codeBlock },
    "markup.link": { fg: link, underline: true },
    "markup.link.url": { fg: link, underline: true },
    comment: { fg: color(theme.syntaxComment ?? theme.textMuted, theme.textMuted as ColorInput) },
    keyword: { fg: color(theme.syntaxKeyword ?? theme.primary, theme.primary as ColorInput) },
    function: { fg: color(theme.syntaxFunction ?? theme.accent ?? theme.primary, code) },
    variable: { fg: color(theme.syntaxVariable ?? theme.text, text) },
    string: { fg: color(theme.syntaxString ?? theme.success, theme.success as ColorInput) },
    number: { fg: color(theme.syntaxNumber ?? theme.warning, theme.warning as ColorInput) },
    type: { fg: color(theme.syntaxType ?? theme.info ?? theme.primary, link) },
    operator: { fg: color(theme.syntaxOperator ?? theme.textMuted, theme.textMuted as ColorInput) },
    punctuation: { fg: color(theme.syntaxPunctuation ?? theme.textMuted, theme.textMuted as ColorInput) },
  })
}
