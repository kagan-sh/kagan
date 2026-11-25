// `kagan/chat`: a chat thread's events, its open approvals, and the prompt box of the one on screen.
// The events are the log's own, the same ones `threads.events` returns.

/** One event of the log, exactly as `threads.events` returns it. */
export interface Event { threadId: string; seq: number; at: number; type: string; data?: unknown }

/** The thread's events after `afterSeq`, exactly `threads.events`: ascending by default, `limit` of them. A message or thought arrives in chunks that share `data.messageId`, and one tool call in several `tool` events that share `data.id`, so fold those. */
export function items(threadId: string, options?: { afterSeq?: number; limit?: number; order?: "asc" | "desc" }): Promise<Event[]>;
/** The next event after `afterSeq`, or null when the wait ended. A null may also mean the open approvals changed: read `approvals()` again either way. At most 60 s. */
export function waitItem(threadId: string, options: { afterSeq: number; timeoutMs: number }): Promise<Event | null>;
/** The open approvals and questions of this chat, oldest first. Never paged. The host draws them and answers them; a plugin reads them and never answers one. */
export function approvals(threadId: string): Promise<{ interactionId: string; title: string; options: { id: string; label: string; role: "primary" | "safe" | "destructive" | null }[] }[]>;
/** Sends the prompt, exactly `kg thread input`. Refused while a turn is open or the agent is not running. At most 1 MiB. */
export function send(threadId: string, text: string): Promise<void>;
/** Ends the reply and keeps the agent, exactly `kg thread cancel`. */
export function cancel(threadId: string): Promise<void>;
/** The prompt box's text, for the chat on screen. */
export function draft(threadId: string): Promise<string>;
/** Replaces the prompt box's text. At most 1 MiB. */
export function setDraft(threadId: string, text: string): Promise<void>;
