# Themes

kagan has three themes: `fjord` (the default), `ember`, and `grey`. Each theme has a light mode and a dark mode. In System mode, kagan follows the light or dark setting of your computer.

## Change the theme

Open Settings → Appearance (Cmd+,). Click a theme card, then select System, Light, or Dark. From a terminal:

```bash
kg theme get
kg theme set fjord dark
kg theme set ember system
```

## Make your own theme

1. Click **New theme…** in Settings → Appearance. kagan copies the current theme to `<id>-copy.json` and opens it in your editor.
2. Change the colors and save the file. The window changes in about one second.

From a terminal, you select the name:

```bash
kg theme new mine
kg theme new mine --from ember
kg theme set mine dark
```

`kg theme folder` shows the folder of your themes, `<data folder>/themes`. The file name is the theme id. Use lowercase letters, digits, and hyphens.

A theme file has a `name`, a `light` palette, and a `dark` palette. Colors use the `#RRGGBB` form. [`docs/themes/ocean.json`](https://github.com/kagan-sh/kagan/blob/main/docs/themes/ocean.json) is a full example. The [look spec](spec/look.md) lists all the colors.

If a file has an error, its card shows the error. If you break the theme that you use, the window keeps the last good colors until you correct the file.

## Themes from plugins

A plugin can add themes. Its id is the plugin id, a slash, and the theme id, for example `kg theme set dev.kagan.theme/midnight dark`. If you turn off the plugin, kagan goes back to the default theme. [`examples/plugins/theme`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins/theme) is an example.
