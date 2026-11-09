// @ts-check
import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { setStatus } from "kagan/host";

let clicks = 0;

function show() {
  setStatus(`${clicks}`, { tone: "quiet", tooltip: "Click to count" });
}

export default class Plugin extends View {
  init(_props, cx) {
    show();
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          if (job.kind === "command" && job.command === "count") {
            clicks += 1;
            show();
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
    return "Status";
  }
}
