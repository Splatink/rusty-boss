# Rusty Boss

A Linux-first, native Rust editor/librarian reconstruction for **BOSS KATANA Gen 3**.
Uses egui for the desktop and ALSA raw MIDI for USB communication; no Wine, browser
engine, vendor executable, or vendor graphics are needed at runtime.

**Status: experimental 0.1 foundation, not full Tone Studio feature parity.**
Both supplied installers were inspected. The packaged JavaScript provides unusually
good static protocol evidence. The user reports the initial implementation works
with their amp; that is not independent verification of every hardware path,
audible result, model/firmware or actual vendor-file interoperability. The latest
UI pass was strictly offline despite the amp being connected. Do not treat this as your
only backup tool. See [the parity checklist](docs/PARITY.md).

## Build and run

From this directory:

```sh
cargo build --locked
cargo run --locked                  # native desktop
cargo test --locked
cargo clippy --all-targets -- -D warnings
```

The development executable is `target/debug/rusty-boss`. To make an optimized build:

```sh
cargo build --release --locked
./target/release/rusty-boss
```

Rust, a C toolchain, pkg-config, ALSA development headers, and a working Linux
OpenGL/EGL + X11 or Wayland desktop are required. Typical build packages:

- Fedora: `gcc gcc-c++ pkgconf-pkg-config alsa-lib-devel libxkbcommon-devel wayland-devel mesa-libEGL-devel`
- Debian/Ubuntu: `build-essential pkg-config libasound2-dev libxkbcommon-dev libwayland-dev libegl1-mesa-dev`

These are instructions, not automatically installed prerequisites.
The dependency versions used here are recorded in `Cargo.lock`.

Headless/offline core build (no graphics or ALSA development dependency):

```sh
cargo test --locked --no-default-features
cargo run --locked --no-default-features -- schema --filter amp
```

Offline GUI without MIDI: `cargo run --no-default-features --features gui`.

## Available now

- **Parameter editor:** the recovered map has 106 blocks / 2,463 instantiated
  parameters: 80 patch, 21 system, 3 temporary/telemetry, 1 setup and 1 command block.
  Patch and system blocks are browsable; dangerous raw command/telemetry editing
  is not exposed. **Standard** now provides a Tone Studio-style amp panel and
  selected-algorithm editor; **Advanced raw** retains the technical view.
- Amp controls, all three effect-color banks, MOD/FX algorithm parameters, delays,
  reverb, EQs, contour, solo, pedal effects, noise suppression, send/return,
  knob/expression/GA-FC assignments, global EQ, USB/line-out and MIDI settings
  in the recovered map. The standard editor covers amp/effect/EQ/solo/contour/
  pedal/SR pages; detailed assignments and system settings still use technical
  controls. Recovering a parameter does not establish its complete workflow.
- **951 recovered numeric UI bindings:** friendly labels, finite value tables,
  nonsequential option mappings and explicit recovered units where known. MOD/FX
  pages follow the selected algorithm and green/red/yellow bank. Unknown formats
  stay numeric; browsing does not reset hidden parameters.
- **Panel-position snapshots:** separate read-only booster/MOD/FX/delay/reverb
  macro readouts, with unavailable values shown as a dash. A panel position of
  50 is **not necessarily** an algorithm Effect Level of 50. These values refresh
  on a patch read, not continuously. See [the investigation](docs/PANEL-VALUES.md).
- **Signal chain:** nine original patterns, pedal/SR/EQ position selectors and a
  clickable ordered diagram (54 base / 216 full routing combinations). No drag/drop.
- Basic model visibility for Bloom and Artist solo delay; offline model preview.
- **Preset protection:** default read-only gate below the UI; separate temporary
  and persistent write permissions. See the workflow below.
- **Local librarian:** rename, notes, add/duplicate/delete/reorder patches;
  Gen 3 `.tsl` import/export. Save livesets explicitly; a multi-library database
  and automatic session recovery are not implemented.
- **ALB files:** load, round-trip, inspect and extract backup patches into the local
  librarian. Missing/short patch blocks are default-filled with a warning count;
  unknown JSON fields and parameter blocks are retained.
- **Experimental MIDI:** port discovery, identity/model check, editor-mode
  handshake, current/system/slot reads, live parameter writes, local patch preview,
  channel selection, current hardware edit-buffer commit, sequential all-data reads.
- Requests validate seven-bit data, 28-bit addresses, checksums, response device
  IDs and complete byte coverage. I/O is paced and runs on a worker thread.
  Failed reads are not published as completed snapshots.

Screenshots: [amp panel](docs/screenshots/editor.png) · [MOD/chorus editor](docs/screenshots/mod.png) · [reverb editor](docs/screenshots/reverb.png).
The refreshed Standard layout uses effect cards, larger ticked controls and
collapsible secondary amp settings; the technical view is still available.

## Strictly read-only hardware diagnostic

```sh
./target/release/rusty-boss ports
./target/release/rusty-boss read-reverb --port hw:3,0,0 > reverb-read.json
```

Use your actual port from `ports`. This bounded probe sends only identity/RQ1
requests, never enters editor mode, never selects a channel, and rejects **all**
DT1 writes—including setup and cleanup—at the transport. It prints raw packets,
read blocks and separate reverb panel/level values; there are no retries.
It is distinct from the GUI's protected editor connection, which permits
editor-mode setup/cleanup. The user-authorized read-only KATANA-50 observation
and its limits are documented in [Panel values](docs/PANEL-VALUES.md).

## Desktop workflow

1. Start **offline**. The initial patch/system values are recovered factory defaults,
   **not** data from your amplifier.
2. Use **Librarian** to enter a TSL file path and import it, or edit the initial patch.
   Use **Editor → Standard** for amp knobs, effect sections and bank/algorithm
   controls. Drag a knob up/down, use arrow keys after focusing it, or click its
   number to type a value. Dropdowns show recovered named/time/frequency values.
   Use **Advanced raw** for all underlying blocks and parameter search.
3. Save your local liveset from **Librarian**. It includes edits to its selected
   patch. The close dialog warns about unsaved local data.
4. For hardware tests, first make a known-good backup using the original program.
5. Under **Connection**, click **Refresh MIDI ports**, select the Katana control
   port and Connect. Startup does not enumerate or connect. The default
   **Protected · reads only** permission allows identity/reads and editor-mode
   setup/cleanup only. It does not select, preview, or commit a channel.
6. **Read current sound** replaces the selected LOCAL patch with a hardware read.
   To edit the amp temporarily, choose **Allow temporary edit-buffer changes**,
   read again and explicitly arm **Live edit buffer**. This still blocks saved
   channel/system writes. Switching pages/patches or importing disarms live mode.
7. **Preview local patch on amp** (Advanced) changes the current sound, not a
   stored slot; it requires temporary permission and confirmation. Validate the
   sound and read back before considering a persistent write.
8. Saved writes stay locked unless you type **ALLOW SAVED WRITES** under
   Connection. **Commit hardware edit buffer** in Librarian then separately
   requires typing its A/B destination. It overwrites what is *currently on the
   amplifier*, not implicitly the local patch. PANEL cannot be committed.
   Leave saved writes locked to protect your presets. System live writes also
   require persistent permission and a prior system read.
9. **Backup** can read the available model channels and system/setup values and
   export an ALB. It is a sequential read, not an atomic device snapshot: leave the
   amp idle while reading. Cancellation/error discards the unfinished backup.
   **Bulk restore is not implemented.**

Keep local copies before import, preview, select or commit. Cancellation of a
write does not roll back the amp. A lost acknowledgement leaves hardware state
unknown; do not retry blindly. Disconnect and normal application shutdown attempt
to leave editor mode, but cannot guarantee cleanup after USB loss, a crash or kill.

Linux must already expose the device as an ALSA MIDI port. This project does not
supply a kernel USB driver. If no ports appear, check the kernel, cable, device
mode and normal sound-device permissions. Do not run the app as root or chmod
sound devices world-writable. Another application may have exclusive ownership
of a raw MIDI port.

## CLI

```sh
rusty-boss ports
rusty-boss schema --filter booster
rusty-boss new my-tones.tsl --name "Rusty Boss"
rusty-boss inspect my-tones.tsl
rusty-boss inspect amplifier.alb
rusty-boss request 20000000 16       # prints an RQ1; NEVER sends it
```

The CLI intentionally exposes no generic hardware-write command.
`new` refuses to overwrite an existing path. Desktop saves require confirmation
and use same-directory temporary files + fsync + atomic rename.

## Scope and provenance

The supplied installers contain **Tone Studio 1.1.0 (build 26)**.
All 385 files in the macOS HTML subtree have identical Windows counterparts after
CRLF normalization. Windows has five additional amp-chain images. Native platform
wrappers were not shown equivalent by this comparison.

See [INVESTIGATION.md](docs/INVESTIGATION.md) for artifact hashes, REA Evidence IDs,
source locations, observations versus design choices, and extraction limitations.
The source artifacts/extracted trees remain in the parent workspace's `analysis/`;
they are not shipped inside the Rust executable. `evidence/` contains the
local comparison manifest and REA bundle (ignored by git due to size).

Regenerate protocol metadata from an extracted, matching HTML tree without
executing vendor JavaScript:

```sh
python3 tools/recover_schema.py "/absolute/path/to/Resources/html"
python3 tools/recover_editor.py "/absolute/path/to/Resources/html"
```

These scripts parse inert declarations and HTML; neither evaluates JavaScript
or uses a native bridge. See [UI-RECONSTRUCTION.md](docs/UI-RECONSTRUCTION.md) for
UI evidence, source limitations and deliberate safety/layout differences. Rebuilding Rusty Boss itself does not require
either installer or the extracted files.

Rusty Boss is independent and is not affiliated with Roland/BOSS. See
[NOTICE.md](NOTICE.md). Original branding, artwork, fonts, proprietary native
libraries and application JavaScript are not part of the new application's runtime.
