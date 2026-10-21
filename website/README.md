# Rusty Boss — companion website

A static companion page for the Rusty Boss app. It presents the project's identity, workflow, and
interaction model — and lets you *try* parts of the feel: theme switching, a live oscilloscope,
draggable knobs, a tap-tempo pad, and a drag-tone-to-channel demo.

**It is not the app.** The page never connects to hardware, reads or writes no amplifier, and uses
no network. Everything interactive is a front-end demo.

## Files

| Path | Purpose |
|---|---|
| `index.html` | Single-page site (hero, modes, workflow, editor demo, library demo, themes, build, notes) |
| `css/style.css` | Puppet/Justice/Kill/Lightning palettes (exact values from `src/gui/theme.rs`) + Russo One `@font-face` |
| `js/main.js` | Theme switching, oscilloscope canvas, knob dragging (pointer + arrow keys + double-click value entry), tap tempo, drag-and-drop channel demo, scroll reveals |
| `assets/fonts/RussoOne-Regular.ttf` | The app's bundled Russo One (SIL OFL 1.1), copied from `assets/fonts/` |

## Run it

Open `index.html` directly in a browser, or serve it:

```sh
python3 -m http.server 8137 --bind 127.0.0.1
# then visit http://127.0.0.1:8137/
```

## Notes

- Default theme is **Puppet**; theme choices persist in `localStorage` and re-apply on load, mirroring the app's "remembered" behavior.
- All colors, the wordmark treatment ("RUSTY" ink / "BOSS" accent), and the typeface match the app's default Puppet theme.
- `prefers-reduced-motion` disables the canvas loop, character rise, and pulse animations.
- No build tooling is required — it is plain HTML/CSS/JS.
