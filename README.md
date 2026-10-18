# Rusty Boss

> **Warning: AI-generated project.** Rusty Boss was developed using AI-generated
> code and is experimental. It has not undergone a comprehensive independent code
> or hardware-safety audit. Bugs could change your amp's sound or overwrite saved
> tones. Make a known-good backup with BOSS Tone Studio before use, review the
> code and behavior carefully, and use it at your own risk.

### Your Katana. Your tones. On Linux.

Rusty Boss is a Linux app for editing **BOSS KATANA amplifiers** and organizing
presets. Shape your sound with amp knobs and effect controls, try downloaded tones,
and save your favorites straight to your amp—without Wine or Windows.

**Currently supports KATANA Gen 3.** MkI and MkII amps and preset conversion are
not supported yet.

![Rusty Boss with the default Puppet theme](docs/screenshots/puppet.png)

## Play first, save when you're ready

1. **Connect your amp by USB.** In **Settings**, refresh the MIDI ports, choose
   the Katana's control port, and click **Connect**.
2. **Import your tones.** Click **Import TSL…** and choose a Gen 3 `.tsl` file.
   You can also drop the file onto the app window.
3. **Choose “Preview & edit.”** Click a tone in **My tones** to hear it on your
   amp. Amp knobs and effects become live after the preview is read back.
   Click another tone to try it. **Your saved channels are unchanged.**
4. **Keep a favorite.** Click **Enable channel writes…** and answer **Yes**.
   Drag a tone from **My tones** onto an **Amp channel**, then answer **Yes** to
   the overwrite confirmation. That's it—no typed unlock phrases and no manual
   buffer-commit steps.
5. Click **Channel writes enabled · Lock** when you're done saving.

Saving a channel changes the amp's current sound and replaces that channel's
previous tone. The app saves the tone you dragged, not an unrelated sound that
happens to be in the amp buffer. **PANEL is not a save destination.**

> **Keep a backup.** Hardware support is still experimental. Make a known-good
> backup with BOSS Tone Studio before overwriting channels, and don't use Rusty
> Boss as your only backup tool. If a save fails or the USB connection drops, the
> amp's state may be unknown—check it rather than blindly retrying.

## Protection that stays visible

The controls at the top tell you what the app is allowed to change:

| Mode | What it does |
|---|---|
| **Read-only** | Browse tones locally and read the amp. Clicking a tone does not change the amp's sound. Preview, live knobs, channel switching and saved-channel writes are blocked. |
| **Preview & edit** | Click a tone to hear it and adjust the current sound. Saved channels remain protected. |
| **Channel writes enabled** | Allows saving tones to channels, with a Yes/No confirmation for each overwrite. |

The app starts in **Read-only** every time. It never connects automatically and
never remembers permission to write. Connecting in read-only still sends the
editor-session setup/cleanup commands needed by the connection; it does not
change tone parameters or select a channel.

## A focused editor

- **Editor:** familiar amp knobs, colored effect banks, algorithm-specific
  controls, EQ, delay, reverb, solo, contour and signal routing. No Raw/Regular
  mode switch.
- **My tones + Amp channels:** a single place to choose, audition and drag-save
  presets. Channel names are read when you connect; available channels follow
  your amp model. Clicking a channel copies it locally in read-only mode; in
  preview mode it switches the amp to that saved sound.
- **Library:** rename tones, keep notes, duplicate, reorder or delete them.
- **Settings:** USB-MIDI connection, appearance, and optional backup tools.
  There is no separate System tab or system-parameter editing screen.

Turn a knob by dragging up/down. Focus it and use arrow keys for small changes,
or click the value to type an exact number. Effect cards and section buttons
open the corresponding controls.

**Copy amp sound** reads the current sound into a new local tone—it does not
replace the downloaded tone you were editing. In **Settings → Backup & channel
reads**, you can copy an individual saved channel without switching the amp,
read a full backup, open/save `.alb` files, and recover individual tones.
Bulk backup restore is not available.

### PANEL support today

The full PANEL workflow is **not implemented yet**. If you select PANEL on the
amp itself, **Copy amp sound** and **Preview & edit** can edit its current sound.
But PANEL isn't a selectable/save target in the app, and saving effect assignments
to PANEL for the next power-on isn't supported. This is an app limitation, not
an amplifier limitation. The effect cards' **PANEL** values are snapshot readouts,
not editable or continuously monitored physical-knob controls.

### Saving files vs. saving channels

- **Save file…** writes your local liveset, including edited tones, to a `.tsl`
  file on your computer. It does **not** save a channel on the amplifier.
- **Drag a tone onto an Amp channel** to save it on the amplifier.
- Importing a TSL replaces the currently open local liveset. Save your work first;
  the app asks before discarding unsaved local edits.
- Local edits are not automatically recovered after a crash. Save files explicitly.

Find compatible downloads on [BOSS Tone Exchange](https://bosstoneexchange.com/gear/katana_gen_3).
Use your browser to download them, then import them into Rusty Boss.

## Four looks, one app

Choose a theme in **Settings → Appearance**. Your choice is remembered.

| Theme | Look |
|---|---|
| **Puppet** — default | Warm copper/orange accents over earth-toned surfaces and controls. |
| **Justice** | Pearl-white surfaces with pale mint/sage accents. The only light theme. |
| **Kill** | Crimson accents and deep red-black surfaces. |
| **Lightning** | Metallic steel-blue surfaces and accents mixed with purple selections and highlights. |

[Justice](docs/screenshots/justice.png) · [Kill](docs/screenshots/kill.png) · [Lightning](docs/screenshots/lightning.png)

## Run on Linux

From a source checkout, with Rust, a C toolchain and ALSA development packages installed:

```sh
cargo build --release --locked
./target/release/rusty-boss
```

The app supports **native Wayland** as well as X11. On a Wayland desktop it
prefers Wayland directly; XWayland is not required. Native file choosers use your
desktop's XDG portal. A working graphical Linux
desktop and ALSA USB-MIDI support are required. See [build instructions](docs/DEVELOPMENT.md)
for distribution packages, offline builds, tests and troubleshooting details.

### Amp not showing up?

- Click **Refresh MIDI ports**; the list is not scanned at startup.
- Check the USB cable and that Linux exposes the Katana as an ALSA MIDI device.
  Choose the **MIDI control port**, not an audio device.
- Close other MIDI editors that might have exclusive access to the port.
- Check your distribution's sound-device permissions. Don't run the app as root
  or make sound devices world-writable.

Rusty Boss controls the amp's own sound engine. It is not an audio driver, an amp
simulator, or a DAW, and it does not configure your system's USB-audio routing.

## About

Rusty Boss is an independent Linux editor, not an official Roland/BOSS product.
We study device behavior and compatible file formats to make the amp useful on
Linux; the goal is a friendly editor, not a one-to-one copy of Tone Studio.
Original vendor artwork, application JavaScript and native libraries are not part
of the app's runtime.

For contributors: [development](docs/DEVELOPMENT.md) · [supported features and gaps](docs/PARITY.md)
· [technical investigation](docs/INVESTIGATION.md) · [preset workflow evidence](docs/PRESET-WORKFLOW.md)
· [notices](NOTICE.md) · [GPLv3 licensing review](docs/LICENSE-REVIEW.md).
