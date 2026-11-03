// @ts-check
import { View } from "gpui-kit";
import { fail, next, reply } from "kagan/jobs";
import { log } from "kagan/host";

let runs = 0;

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        try {
          if (job.kind === "schedule") {
            runs += 1;
            log("info", `${job.name} run ${runs}`);
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
    return "Schedule";
  }
}
