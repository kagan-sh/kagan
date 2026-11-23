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

   `"follows": "thread"` is what gives the view `context().props.threadId`. Without it the view has
   no thread and can read nothing. The person picks the plugin in Settings → Installed → Replace
   parts, or approves `kg plugin use chat <id>`.

   ```js
   import { approvals, items, setDraft, waitItem } from "kagan/chat";

   const current = () => (context().props || {}).threadId;

   export class Transcript extends View {
     init(_props, cx) {
       this.cx = cx;
       this.rows = [];
       this.waiting = [];
       this.thread = null;
       this.read();
       cx.spawn(async () => {
         for (;;) {
           await onContextChanged();
           this.read();
         }
       });
     }

     render() { return this.rows.map(draw); }

     /** Starts again when another chat comes on screen; the old read stops on its own. */
     read() {
       const id = current();
       if (!id || id === this.thread) return;
       this.thread = id;
       this.rows = [];
       this.waiting = [];
       this.cx.notify();
       follow(id, this).catch((error) => {
         this.rows = [{ kind: "notice", severity: "error", text: String(error) }];
         this.cx.notify();
       });
     }

     /** Adds a page and replaces the open approvals, keeping the last 200 rows. */
     take(page, asked) {
       this.rows = [...this.rows, ...page].slice(-200);
       if (asked) this.waiting = asked;
       this.cx.notify();
     }
   }

   // Read the log forward, then wait for the next item, while that chat is the one on screen.
   async function follow(id, view) {
     let after = 0;
     while (current() === id) {
       const [page, asked] = await Promise.all([
         items(id, { afterSeq: after, order: "asc" }),
         approvals(id),
       ]);
       if (current() !== id) return;
       for (const item of page) after = Math.max(after, item.seq);
       view.take(page, asked);
       const item = await waitItem(id, { afterSeq: after, timeoutMs: 30_000 });
       if (current() !== id) return;
       if (item) { after = Math.max(after, item.seq); view.take([item]); }
     }
   }
   ```

   An item is `{ seq, threadId, at }` plus one `kind`: `message` `{ role, text, messageId }`, where
   chunks share one `messageId`; `work` `{ work: { id, tool, title, status, args?, output? } }`;
   `notice` `{ severity, text }`; and `turn` `{ outcome, error? }`. An item is one event, not one
   row: one tool call is a `work` item that starts it and another that ends it, with `title: ""` on
   the end, so fold them by `work.id`, where the last non-empty title and the last status win.
   `thought` and `plan` land with the events that fill them. The open approvals and questions are
   `approvals(id)` — `{ interactionId, title, options }[]`, oldest first, never paged — and the host
   draws and answers them, so a plugin keeps them by `interactionId` and never draws the buttons.

   | want | do |
   |---|---|
   | draw the whole transcript | `"replaces": "chat"` |
   | one kind of row, the rest kagan's | not yet: the part names land with the transcript's thought, plan and approval rows |
   | a button beside Send | `{ "prompt": true, "when": "thread" }`, at most four a manifest |
   | a button on the thread's title | `"header": true` |
   | a new thread | `threads.spawn`, which needs `"areas": ["threads"]` |

   Caps, all refused with a sentence that names the number: 200 items a call, 256 KiB a page (or one
   item over it, alone), 1 MiB a prompt, 1 MiB a draft write, 60 s a wait, 50 ms a render, 16 ms a
   slow row. `send(id, text)` is exactly
   `kg thread input`: it is refused while a turn is open or the agent is not running. `cancel(id)` is
   exactly `kg thread cancel`. `draft(id)` reads the prompt box and `setDraft(id, text)` replaces it,
   only for the chat on screen.

   Safety: draw agent text with `Markdown`, never a remote image and only an http or https link,
   keep no dialog or popup in a replacement, send heavy work to `process.run`, and never
   answer an approval or a question. Read `references/manifest.md` for the rest of the manifest and
   `references/ui-controls.md` for the components.
