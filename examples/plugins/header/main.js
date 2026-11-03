import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log, toast } from "kagan/host";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          if (job.kind === "command" && job.command === "handoff") {
            await toast(`Hand off ${job.threadId}`);
            log("info", "handoff ran");
            reply(job.id, null);
          } else {
            fail(job.id, "Unknown job");
          }
        } catch (error) {
          fail(job.id, String(error));
        }
      }
    });
  }
  render() {
    return "Header";
  }
}
