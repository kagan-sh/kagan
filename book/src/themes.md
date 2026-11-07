# Themes

The window ships with three themes: `fjord` (the default), `ember`, and `grey`. Each has a light mode and a dark mode. **System**, the default mode, follows your computer's light or dark setting. A plugin can add more themes, and you can write your own file.

## Change the theme

Open Settings → Appearance (Cmd+,). Click a theme card, then select System, Light, or Dark.

```bash
kg theme get
kg theme set fjord dark
kg theme set ember system
```

## Make your own theme

1. Click **New theme…** in Settings → Appearance.
2. Change the colors in the file that opens, and save.

kagan copies the current theme to `<id>-copy.json`. The window changes about one second after you save.

From a terminal, you pick the name:

```bash
kg theme new mine
kg theme new mine --from ember
kg theme set mine dark
```

`kg theme folder` prints `<data folder>/themes`. The file name is the theme id. Use lowercase letters, digits, and hyphens.

A theme file has a `name`, a `light` palette, and a `dark` palette. Colors are `#RRGGBB`. An optional `highlight` object sets code colors for the review and the Editor, with `light` and `dark` in Zed's syntax format. Without it, kagan keeps the default code colors. An optional `look` object sets density and type, in pixels: the spacing steps, the four corner radii, the text sizes, and the heights of a list row and a control row. Without `look`, kagan keeps the current numbers. Plugin web panels get the same spacing and sizes as CSS variables. [`docs/themes/ocean.json`](https://github.com/kagan-sh/kagan/blob/main/docs/themes/ocean.json) is a full example.

A bad file shows the error on its card. If you break the theme you are using, the window keeps the last good colors until you correct the file.

## Themes from plugins

A plugin can add themes. The id is the plugin id, a slash, and the theme id: `kg theme set dev.kagan.theme/midnight dark`. If you disable the plugin, kagan returns to the default theme. [`examples/plugins/theme`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins/theme) is an example.
