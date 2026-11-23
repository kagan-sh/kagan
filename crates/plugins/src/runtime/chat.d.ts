// `kagan/chat`: a chat thread's transcript, its open approvals, and the prompt box of the one on screen.
// Every kind is typed here. `items` returns the ones this version writes: message, work, notice and
// turn. `thought` and `plan` are added with the events that fill them.

export type ChatItem = { seq: number; threadId: string; at: number } & (
  | { kind: "message"; role: "user" | "agent"; text: string; messageId: string | null }
  | { kind: "work"; work: {
        id: string;
        tool: "read" | "edit" | "delete" | "move" | "search" | "execute" | "think" | "fetch" | "other";
        /** The name on the event that starts a call, and "" on the one that ends it. */
        title: string;
        /** The state this event leaves the call in. */
        status: "running" | "ok" | "error";
        args?: unknown; output?: string;
      } }
  | { kind: "notice"; severity: "info" | "warning" | "error"; text: string }
  | { kind: "turn"; outcome: "completed" | "interrupted" | "failed"; error?: string }
);

/** Up to `limit` items after `afterSeq`, newest first unless `order` is "asc". 100 by default, 200 at most, 256 KiB a page or one item over it alone. An item is one event, and one tool call is two or more `work` items with one `work.id`: fold them by `id`, where the last non-empty title and the last status win. */
export function items(threadId: string, options?: { afterSeq?: number; limit?: number; order?: "asc" | "desc" }): Promise<ChatItem[]>;
/** The next item after `afterSeq`, or null when the wait ended. A null may also mean the open approvals changed: read `approvals()` again either way. At most 60 s. */
export function waitItem(threadId: string, options: { afterSeq: number; timeoutMs: number }): Promise<ChatItem | null>;
/** The open approvals and questions of this chat, oldest first. Never paged. The host draws them and answers them; a plugin reads them and never answers one. */
export function approvals(threadId: string): Promise<{ interactionId: string; title: string; options: { id: string; label: string; role: "safe" | "destructive" | null }[] }[]>;
/** Sends the prompt, exactly `kg thread input`. Refused while a turn is open or the agent is not running. At most 1 MiB. */
export function send(threadId: string, text: string): Promise<void>;
/** Ends the reply and keeps the agent, exactly `kg thread cancel`. */
export function cancel(threadId: string): Promise<void>;
/** The prompt box's text, for the chat on screen. */
export function draft(threadId: string): Promise<string>;
/** Replaces the prompt box's text. At most 1 MiB. */
export function setDraft(threadId: string, text: string): Promise<void>;
