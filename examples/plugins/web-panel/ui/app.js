const line = document.getElementById("line");

window.addEventListener("kagan", (event) => {
  const detail = event.detail;
  line.textContent =
    detail && detail.from === "plugin"
      ? "plugin heard " + JSON.stringify(detail.echo)
      : JSON.stringify(detail);
});

function paint() {
  const bg = getComputedStyle(document.documentElement)
    .getPropertyValue("--bg")
    .trim();
  document.body.dataset.bg = bg;
}

const theme = document.getElementById("kagan-theme");
if (theme) theme.addEventListener("load", paint);
paint();

window.kagan.post({ from: "page", text: "hello" });
