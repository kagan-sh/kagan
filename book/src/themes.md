# Themes

The app starts on the `fjord` theme, in system mode: it follows macOS, Windows or Linux between light and dark, and changes live when the system does. `fjord` is Ember Fjord: a cool graphite with one ember accent.

The Appearance page in Settings shows the themes and the mode. Press Cmd+, (Ctrl+Shift+, off macOS), or the gear at the bottom of the rail. Each theme is a card with three of its colors and where it comes from: Built in, Your file, or a plugin's name. The one in use has a brand-accent border and a check mark. Under Mode, choose System, Light or Dark. The same choices are on the command line:

```bash
kg theme get
kg theme set fjord light
kg theme set fjord dark
kg theme set fjord system
```

`system` is the default. `light` and `dark` pin the mode, and then the system setting is ignored. Changes from the system and from `kg` apply at once.

`ember` and `grey` ship with the app too. Run `kg theme set ember dark` or `kg theme set grey dark` to pick one. Their files are [`docs/themes/ember.json`](https://github.com/kagan-sh/kagan/blob/main/docs/themes/ember.json) and [`docs/themes/grey.json`](https://github.com/kagan-sh/kagan/blob/main/docs/themes/grey.json).

## Your own theme

A custom theme is a JSON file at `<dataDir>/themes/<id>.json`. `KG_DATA_DIR` overrides the data directory. Otherwise it is `~/.kagan`, or `%USERPROFILE%\.kagan` on Windows.

The id is the file name without `.json`. Use lowercase letters, digits, and dashes. `fjord`, `ember` and `grey` are reserved.

The quickest start is **New theme…** in the Theme row. It writes a copy of the theme in use to the themes folder, as `<id>-copy.json`, and opens it in your default editor. **Open themes folder** opens the folder. From the command line:

```bash
kg theme new mine              # copies the theme in use to themes/mine.json
kg theme new mine --from ember # copies ember instead
kg theme folder                # prints the folder
kg theme set mine dark
```

`kg theme new` never overwrites a file, and refuses a built-in id. You can also copy [`docs/themes/ocean.json`](https://github.com/kagan-sh/kagan/blob/main/docs/themes/ocean.json) yourself. The file has a `name`, a full `light` palette, and a full `dark` palette. Every token is required except `hover` and `added`. The `terminal` object is optional. Colors are `#RRGGBB`. Unknown fields, missing fields, and blank names are errors. The file can be at most 64 KiB, and the name at most 64 characters on one line.

Saving the file updates the window within a second. A file that does not load shows its error on its card. If you break the theme you are using, the window keeps its last good colors and says so once; fix the file and it applies. If the selected theme is missing, the Appearance page says so and the window uses the default palette for the mode you picked. The id stays selected, so a restored file applies again.

## Themes from plugins

A plugin can ship themes. Each is a file in the plugin folder, in the same format, listed in its `kagan.json`:

```json
"themes": [{ "id": "midnight", "file": "themes/midnight.json" }]
```

Its card shows the plugin's name, and its id is the plugin id, a slash, and the theme id: `kg theme set dev.kagan.theme/midnight dark`. A theme needs no powers. `kg plugin check` reads each theme file and names the one that fails. Turning the plugin off or removing it removes its themes; if you were using one, the window says so once and uses the default. Turn the plugin on again and the theme comes back. [`examples/plugins/theme`](https://github.com/kagan-sh/kagan/tree/main/examples/plugins/theme) is a plugin with one theme.

The tokens and the window are in the [look spec](spec/look.md). The window uses Inter at 13 px.
