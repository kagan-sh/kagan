// @ts-check
import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log } from "kagan/host";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          if (job.kind === "instruct") {
            log("info", `instruct for ${job.threadId}`);
            reply(job.id, { text: `Adopt the reviewer persona for thread ${job.threadId}.` });
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
    return "Instruct";
  }
}
