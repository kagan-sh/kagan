# Draw or extend a chat

A chat thread draws its transcript in the window. A plugin may draw that body, and it may add a
button to the prompt row. It never draws the prompt box, Send, Cancel, the strip, the ended line or
an approval, and it never answers one.

   ```json
   "powers": { "areas": ["chat"] },
   "ui": { "panels": [{ "id": "transcript", "title": "Chat", "view": "Transcript",
                        "placement": "center", "replaces": "chat", "follows": "thread" }] },
   "commands": [{ "id": "improve", "title": "Improve", "when": "thread", "prompt": true }]
   ```

   `"follows": "thread"` is what gives the view `context().props.threadId`; without it the view has
   no thread and can read nothing. The person picks the plugin in Settings → Installed → Replace
   parts, or approves `kg plugin use chat <id>`.

   ```js
   import { View, div } from "gpui-kit";
   import { v_flex } from "gpui-base";
   import { Markdown } from "gpui-component";
   import { context, onContextChanged } from "kagan/host";
   import { cancel, draft, items, send, setDraft, waitItem } from "kagan/chat";

   // One row per item, yours to style; agent text goes through Markdown. A tool call arrives as two
   // `work` items with one `work.id`, so fold them by it, and keep the last 200 rows.
   function keep(view, page) {
     for (const item of page) {
       const row = item.kind === "work"
         ? view.rows.find((r) => r.kind === "work" && r.work.id === item.work.id)
         : undefined;
       if (row) row.work = { ...row.work, ...item.work, title: item.work.title || row.work.title };
       else view.rows.push(item);
     }
     view.rows = view.rows.slice(-200);
     view.cx.notify();
   }

   const draw = (item) =>
     item.kind === "message" ? new Markdown(`chat-${item.seq}`, item.text)
     : div().child(
         item.kind === "thought" ? item.text
         : item.kind === "plan" ? item.plan.steps.map((step) => step.step).join(" · ")
         : item.kind === "work" ? `${item.work.status}: ${item.work.title}`
         : item.kind === "notice" ? `${item.severity}: ${item.text}`
         : `The turn ${item.outcome}.`,
       );

   const shown = () => (context().props || {}).threadId;

   export class Transcript extends View {
     init(_props, cx) {
       this.cx = cx;
       this.rows = [];
       this.thread = null;
       this.read();
       cx.spawn(async () => {
         for (;;) {
           await onContextChanged();
           this.read();
         }
       });
     }

     render() { return v_flex().gap_2().children(this.rows.map(draw)); }

     /** Starts again when another chat comes on screen; the old read stops on its own. */
     read() {
       const id = shown();
       if (!id || id === this.thread) return;
       this.thread = id;
       this.rows = [];
       this.cx.notify();
       follow(id, this).catch((error) => {
         keep(this, [{ kind: "notice", severity: "error", text: String(error) }]);
       });
     }
   }
   // Read the log forward, then wait for the next item, while that chat is the one on screen.
   async function follow(id, view) {
     let after = 0;
     while (shown() === id) {
       const page = await items(id, { afterSeq: after, order: "asc" });
       if (shown() !== id) return;
       for (const item of page) after = Math.max(after, item.seq);
       keep(view, page);
       const item = await waitItem(id, { afterSeq: after, timeoutMs: 30_000 });
       if (shown() !== id) return;
       if (item) { after = Math.max(after, item.seq); keep(view, [item]); }
     }
   }
   ```

   The prompt box and the reply, from a `"prompt": true` command's job, where `job.threadId` is the chat on screen:

   ```js
   const text = await draft(job.threadId);
   await setDraft(job.threadId, `Improve this:\n\n${text}`);
   await send(job.threadId, text); // the prompt; refused while a turn is open or the agent is not running
   await cancel(job.threadId); // ends the reply, keeps the agent; refused when no turn is open
   ```

   Every item kind, its fields, the caps and the fold rules are typed in `chat.d.ts`, which
   `kg plugin types` writes. Every call names a chat: a visible, unarchived `acp` thread. Any other
   thread is refused with `thread <id> is not a chat; its transcript is its terminal`.

   The open approvals and questions are `approvals(id)` —
   `{ interactionId, title, options: { id, label, role }[] }[]`, oldest first, never paged — where
   `role` is `"primary"`, `"safe"`, `"destructive"` or `null` and says how the host draws that
   option. The host draws and answers them, so a plugin keeps them by `interactionId` and never
   draws the buttons.

   A `"prompt": true` command is a button beside Send, at most four a manifest, and `"header": true`
   puts one on the thread's title. A command's `description` (one plain line of at most 200
   characters) is the button's tooltip and falls back to the title, so a prompt button says what a
   click does to the draft before it is clicked. Drawing one kind of row and leaving the rest to kagan lands with
   `chat.message`, `chat.thought`, `chat.work`, `chat.plan` and `chat.approval`. A new thread is
   `threads.spawn`, which needs `"areas": ["threads"]`.

   Safety: draw agent text with `Markdown`, never a remote image and only an http or https link,
   keep no dialog or popup in a replacement, and send heavy work to `process.run`. Read
   `references/manifest.md` for the rest of the manifest and `references/ui-controls.md` for the
   components.
