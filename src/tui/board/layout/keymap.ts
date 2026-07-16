export function keymapKey(key: string): string | { name: string } {
  if (key === ",") return { name: "," }
  if (key === "?") return "?,shift+/"
  if (key === "return") return "return,enter"
  return key
}
