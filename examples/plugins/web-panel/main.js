import { View } from "gpui-kit";
import { next, reply, fail } from "kagan/jobs";
import { post } from "kagan/panel";

export default class Plugin extends View {
  init(_props, cx) {
    cx.spawn(async () => {
      for (;;) {
        const job = await next();
        handle(job).then(
          (value) => reply(job.id, value),
          (error) => fail(job.id, String(error)),
        );
      }
    });
  }

  render() {
    return "";
  }
}

async function handle(job) {
  if (job.kind === "page") {
    await post(job.panelId, { from: "plugin", echo: job.message });
    return null;
  }
  throw new Error(`Unknown job ${job.kind}`);
}
