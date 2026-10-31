import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        if (job.kind === "cli" && job.command === "hi") {
          reply(job.id, { exitCode: 0, stdout: "Hello\n", stderr: "" });
        } else {
          fail(job.id, "Unknown job");
        }
      }
    });
  }
  render() {
    return "CLI";
  }
}
