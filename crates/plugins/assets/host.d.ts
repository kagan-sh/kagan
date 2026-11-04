/** `overlay` is the size in pixels of the front window's center, which a `ui.overlay` view covers. That view may only paint: boxes, text, images, svgs and paths with styles and motion; a handler, hover style, cursor, scroll area, other component or window call fails it. */
export function context(): { threadId?: string; projectId?: string; focused: boolean; theme: 'light' | 'dark'; props?: unknown; overlay?: { width: number; height: number } };
export function onContextChanged(): Promise<ReturnType<typeof context>>;
export function threadOnScreen(threadId: string): boolean;
export function openThread(threadId: string): Promise<void>;
/** Shows a panel this plugin declares, placed left, center or right. Resolves when it is on screen. */
export function openPanel(panelId: string, props?: unknown): Promise<void>;
/** Opens a URL in the system browser. Needs `powers.openUrl`. */
export function openUrl(url: string): Promise<void>;
export function toast(message: string, action?: { label: string; command: string }): Promise<void>;
/** Adds a note to the review on screen, when this plugin draws it. kagan's footer sends or discards it. */
export function addComment(comment: { path: string; side?: 'old' | 'new'; start: number; end: number; text: string }): Promise<void>;
/** Pastes text into a thread's program terminal, without pressing Enter. At most 1 MiB. */
export function pasteIntoThread(threadId: string, text: string): Promise<void>;
/** Writes a PNG to the clipboard. `pngBase64` is standard base64, at most 8 MiB decoded. Needs `powers.clipboard.write`. On Linux it fails: png clipboard is not available on this platform. */
export function writeClipboardPng(pngBase64: string): void;
