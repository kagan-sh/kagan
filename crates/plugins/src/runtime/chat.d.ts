// `kagan/chat`: a chat thread's transcript and the prompt box of the one on screen.
// Every kind is typed here. `items` returns the ones this version writes: message, work, notice,
// turn and approval. `thought` and `plan` map when their events land.

export type ChatItem = { seq: number; threadId: string; at: number } & (
  | { kind: "message"; role: "user" | "agent"; text: string; messageId: string | null }
  | { kind: "work"; work: {
        id: string;
        tool: "read" | "edit" | "delete" | "move" | "search" | "execute" | "think" | "fetch" | "other";
        title: string; status: "running" | "ok" | "error";
        /** Raw input, as the agent sent it. Cut at 4 KiB. */
        args?: unknown; argsTruncated?: boolean;
        /** Result text. Cut at 8 KiB. */
        output?: string; outputTruncated?: boolean;
      } }
  | { kind: "notice"; severity: "info" | "warning" | "error"; text: string }
  | { kind: "turn"; outcome: "completed" | "interrupted" | "failed"; error?: string }
  | { kind: "approval"; approval: {
        interactionId: string; title: string;
        options: { id: string; label: string; role: "safe" | "destructive" | null }[];
      } }
  | { kind: "thought"; text: string; done: boolean }
  | { kind: "plan"; plan: { steps: { step: string; status: "pending" | "running" | "done" }[] } }
);

/** Up to `limit` items after `afterSeq`, newest first unless `order` is "asc". 100 by default, 200 at most, 256 KiB a page. */
export function items(threadId: string, options?: { afterSeq?: number; limit?: number; order?: "asc" | "desc" }): Promise<ChatItem[]>;
/** The next item after `afterSeq`, or null when the wait ended. A null may also mean the open approvals changed: either way, read `items()` again. */
export function waitItem(threadId: string, options: { afterSeq: number; timeoutMs: number }): Promise<ChatItem | null>;
/** Sends the prompt, exactly `kg thread input`. Refused while a turn is open or the agent is not running. At most 1 MiB. */
export function send(threadId: string, text: string): Promise<void>;
/** Ends the reply and keeps the agent, exactly `kg thread cancel`. */
export function cancel(threadId: string): Promise<void>;
/** The prompt box's text, for the chat on screen. */
export function draft(threadId: string): Promise<string>;
/** Replaces the prompt box's text. At most 1 MiB. */
export function setDraft(threadId: string, text: string): Promise<void>;
