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
   import { draft, items, setDraft, waitItem } from "kagan/chat";

   export class Transcript extends View {
     init(_props, cx) {
       this.rows = [];
       cx.spawn(async () => {
         let value = context();
         for (;;) {
           const id = value && value.threadId;
           if (id) await follow(id, (page) => { this.rows = page; cx.notify(); });
           value = await onContextChanged();
         }
       });
     }

     render() { return v_flex().gap_2().children(this.rows.map(draw)); }
   }

   // Read the log forward, then wait for the next item.
   async function follow(id, show) {
     let after = 0;
     for (;;) {
       const page = await items(id, { afterSeq: after, order: "asc" });
       for (const item of page) after = Math.max(after, item.seq);
       if (page.length > 0) show(page);
       const item = await waitItem(id, { afterSeq: after, timeoutMs: 30_000 });
       if (item) { after = Math.max(after, item.seq); show([item]); }
     }
   }
   ```

   An item is `{ seq, threadId, at }` plus one `kind`: `message` `{ role, text, messageId }`, where
   chunks share one `messageId`; `work` `{ work: { id, tool, title, status, args?, output? } }` for
   one tool call; `notice` `{ severity, text }`; `turn` `{ outcome, error? }`; and `approval`
   `{ approval: { interactionId, title, options } }`. `thought` and `plan` are typed and map when the
   transcript draws them. The open approvals and questions follow every page, so draw them at the
   end, where the turn waits, and keep them by `interactionId`.

   | want | do |
   |---|---|
   | draw the whole transcript | `"replaces": "chat"` |
   | one kind of row, the rest kagan's | not yet: the part names land with the transcript's thought, plan and approval rows |
   | a button beside Send | `{ "prompt": true, "when": "thread" }`, at most four a manifest |
   | a button on the thread's title | `"header": true` |
   | a new thread | `threads.spawn`, which needs `"areas": ["threads"]` |

   Caps, all refused with a sentence that names the number: 200 items a call, 256 KiB a page, 1 MiB
   a prompt, 1 MiB a draft write, 50 ms a render, 16 ms a slow row. `send(id, text)` is exactly
   `kg thread input`: it is refused while a turn is open or the agent is not running. `cancel(id)` is
   exactly `kg thread cancel`. `draft(id)` reads the prompt box and `setDraft(id, text)` replaces it,
   only for the chat on screen.

   Safety: draw agent text with `Markdown`, never a remote image and never an http link the agent
   chose, keep no dialog or popup in a replacement, send heavy work to `process.run`, and never
   answer an approval or a question. Read `references/manifest.md` for the rest of the manifest and
   `references/ui-controls.md` for the components.
