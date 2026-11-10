# Job loop and timing

## Job loop

```js
// @ts-check
import { View } from "gpui-kit";
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
```

| kind | you receive | you reply |
|---|---|---|
| `command` | `{ command, threadId?, projectId? }` | `null` |
| `tool` | `{ name, input, threadId }` | `{ text, isError? }` |
| `cli` | `{ command, argv, cwd, threadId?, projectId? }` | `{ exitCode, stdout, stderr }` |
| `event` | `{ type, threadId, projectId, data }` | nothing |

A tool or CLI job has 120 seconds. A command or event has 10. The script shares the UI thread: keep a render under 50 ms and a job turn under 500 ms. Longer work goes to `kagan/process` `run`.

`job.command` on a CLI job is the subcommand (`kg weather today sf` arrives as `command: "today"`, `argv: ["sf"]`). Declare every subcommand in the manifest or the call never reaches the plugin.
