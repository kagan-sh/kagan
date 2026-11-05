# Themes

kagan has three themes: `fjord` (the default), `ember`, and `grey`. Each theme has a light mode and a dark mode. In System mode, kagan uses your computer's light or dark setting.

## Change the theme

Open Settings → Appearance (Cmd+,). Click a theme card, then select System, Light, or Dark.

From a terminal:

```bash
kg theme get
kg theme set fjord dark
kg theme set ember system
```

## Make your own theme

1. Click **New theme…** in Settings → Appearance.
2. Change the colors in the file that opens, then save it.

kagan copies the current theme to `<id>-copy.json`. The window changes about one second after you save.

From a terminal, you select the name:

```bash
kg theme new mine
kg theme new mine --from ember
kg theme set mine dark
```

`kg theme folder` shows `<data folder>/themes`, the folder of your themes. The file name is the theme id. Use lowercase letters, digits, and hyphens.

A theme file has a `name`, a `light` palette, and a `dark` palette. Colors use the `#RRGGBB` form. An optional `highlight` object gives the review and the Editor their code colors, with `light` and `dark` in Zed's syntax format; a theme without it keeps the default code colors. An optional `look` object sets the density and the type: the spacing steps, the three corner radii, the text sizes, and the heights of a list row and a control row, all in pixels. A theme without `look` keeps the current numbers, and plugin web panels get the same spacing and sizes as CSS variables. [`docs/themes/ocean.json`](https://github.com/kagan-sh/kagan/blob/main/docs/themes/ocean.json) is a full example.

If a file has an error, its card shows the error. If you break the theme that you use, the window keeps the last good colors until you correct the file.

## Themes from plugins

A plugin can add themes. Its id joins the plugin id and the theme id with a slash, for example `kg theme set dev.kagan.theme/midnight dark`. If you disable the plugin, kagan returns to the default theme. [`examples/plugins/theme`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins/theme) is an example.
