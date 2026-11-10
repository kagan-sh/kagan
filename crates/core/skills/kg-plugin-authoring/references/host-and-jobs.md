# Host calls and the job loop

   ```js
   // @ts-check
   import { View, div } from "gpui-kit";
   import { KgButton } from "gpui-component";
   import { log } from "kagan/host";
   import { next, reply, fail } from "kagan/jobs";

   export default class Plugin extends View {
     init(_props, cx) {
       cx.spawn(async () => {
         for (;;) {
           const job = await next();
           handle(job).then((value) => reply(job.id, value), (error) => fail(job.id, String(error)));
         }
       });
     }
   }

   export class HelloPanel extends View {
     init() { this.count = 0; }
     render() {
       return div().child(
         new KgButton("count")
           .label(`Count ${this.count}`)
           .on_click((_event, cx) => { this.count++; cx.notify(); }),
       );
     }
   }

   async function handle(job) {
     if (job.kind === "command" && job.command === "hello") {
       log("info", "hello from the starter plugin");
       return null;
     }
     throw new Error(`Unknown job ${job.kind}`);
   }
   ```

   Job kinds the loop receives, and the reply each one expects:
   - `command` `{ command, threadId?, projectId? }`: reply `null`.
   - `tool` `{ name, input, threadId }`: reply `{ text, isError? }`.
   - `event` `{ type, threadId, projectId, data }`: reply nothing.
   - `cli` `{ command, argv, cwd, threadId?, projectId? }`: reply `{ exitCode, stdout, stderr }`.

   Host calls come from `kagan/host` (`log`, `openPanel`, `openThread`, `toast`, `openUrl`), `kagan/threads`, and the other `kagan/*` modules named by `powers.areas`. `threads.send(id, text)` needs the `threads` area. It types the text and presses Enter in any running program, at most 1 MiB, into whatever that program is reading now, and writes a user `message` only after the write succeeds. It does not queue. Empty text, control-only text, and a program that is not running are refused. `process.dataDir` is this plugin's data folder (read-write through `fs/promises`); `process.env` is empty.

   Files under that folder need no extra power. A read outside it needs `powers.read` or a `folder` setting.

   `process.run(program, args)` finds `program` on the login-shell PATH (on Windows, the app's own PATH). It does not add the directory of the running binary. `powers.run` of `kg` fails with `program kg not found` unless that PATH already contains `kg`. On macOS the app binary is `/Applications/kagan.app/Contents/MacOS/kg`, and that directory is not on the login-shell PATH. When `kg` is found, `KG_DATA_DIR` and `KG_SOCKET` are passed only if this app was started with them, and the child then talks to this app.
