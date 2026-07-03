import { defineConfig } from "vitepress"

export default defineConfig({
  title: "Kagan",
  description: "Supervision board for AI coding agents inside OpenCode",
  themeConfig: {
    nav: [
      { text: "Quickstart", link: "/quickstart" },
      { text: "Reference", link: "/reference/configuration" },
    ],
    sidebar: [
      { text: "What is Kagan?", link: "/" },
      { text: "Quickstart", link: "/quickstart" },
      {
        text: "Concepts",
        items: [
          { text: "Task lifecycle", link: "/concepts/task-lifecycle" },
          { text: "Isolation", link: "/concepts/isolation" },
          { text: "Choosing a mode", link: "/concepts/choosing-a-mode" },
          { text: "Trust packets", link: "/concepts/trust-packets" },
        ],
      },
      {
        text: "Reference",
        items: [
          { text: "Configuration", link: "/reference/configuration" },
          { text: "Keybindings", link: "/reference/keybindings" },
        ],
      },
      { text: "Troubleshooting", link: "/troubleshooting" },
    ],
    search: { provider: "local" },
    socialLinks: [{ icon: "github", link: "https://github.com/kagan-sh/kagan" }],
  },
})
