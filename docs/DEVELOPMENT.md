# Building and contributing

Rusty Boss is a native Rust/egui Linux editor for KATANA Gen 3. The user guide is
[README.md](../README.md). Device protocol and file-format investigation lives in
[INVESTIGATION.md](INVESTIGATION.md); it is not required to run the app.

## Build

Install Rust 1.99.1 or newer, a C toolchain, pkg-config, ALSA headers and a working X11 or Wayland
desktop with OpenGL/EGL support. Typical distribution packages:

- Fedora: `gcc gcc-c++ pkgconf-pkg-config alsa-lib-devel libxkbcommon-devel wayland-devel mesa-libEGL-devel`
- Debian/Ubuntu: `build-essential pkg-config libasound2-dev libxkbcommon-dev libwayland-dev libegl1-mesa-dev`

The native file chooser uses `rfd`'s XDG portal backend. Install/configure the
portal backend appropriate for your desktop (e.g. GNOME, KDE or GTK) if dialogs
are unavailable. This project does not install a kernel USB driver.

```sh
cargo build --release --locked
./target/release/rusty-boss
# Development:
cargo run --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
cargo fmt --check
```

Other build configurations:

```sh
# No graphics or ALSA dependency:
cargo test --locked --no-default-features
cargo run --locked --no-default-features -- schema --filter amp
# Offline desktop, with no MIDI backend:
cargo run --locked --no-default-features --features gui
cargo clippy --locked --no-default-features --features gui --all-targets -- -D warnings
```

Both `wayland` and `x11` eframe/winit features are enabled. The normal winit
backend selection prefers `WAYLAND_DISPLAY`/`WAYLAND_SOCKET` over `DISPLAY`;
there is no forced X11 backend in the app. Native Wayland doesn't require
XWayland. For an explicit manual Wayland launch from a session with a working
Wayland socket:

```sh
env -u DISPLAY ./target/release/rusty-boss
```

No original installer, vendor app, vendor JavaScript or vendor native bridge is
needed to build or run Rusty Boss. The original graphics and fonts are not used.

## Rust and dependency baseline

The 1.0.0 baseline uses Rust edition 2024 and Rust/Cargo 1.99.1, the current
stable release checked on 2026-10-19. `rust-version` is the supported minimum,
not an upper limit; newer stable compilers are welcome. No exact toolchain is
forced on contributors.

Direct dependencies were checked against crates.io and updated to their latest
stable releases. `Cargo.lock` records the complete resolved graph; build releases
with `--locked`. Transitive versions follow upstream compatibility constraints;
do not force incompatible versions with patches just to remove older numbers.
When updating dependencies, rerun the feature-matrix checks and refresh the
license inventory/notices. See [VALIDATION.md](VALIDATION.md) for release checks.

## Architecture

- `src/gui.rs`: editor/library/settings shell, snapshot-based preview/save requests,
  MIDI worker, latest-wins pending previews, and permission controls.
- `src/gui/classic.rs`: amp/effect controls and routing presentation.
- `src/gui/wordmark.rs`: restrained two-tone display typography using the bundled
  Russo One font through egui's normal antialiased, DPI-aware text pipeline. It
  rerasterizes on monitor/scale changes instead of enlarging a cached image or
  manually scaling a mesh; body fonts are unchanged. Font provenance and the SIL
  OFL are in `assets/fonts/`, with the license also readable in Settings.
- `src/gui/theme.rs`: four palettes and appearance-only persistence in
  `$XDG_CONFIG_HOME/rusty-boss/appearance.json` (or `~/.config/...`). Hardware
  permissions and connections are never persisted.
- `src/session.rs`: validated parameter reads/writes, temporary preview, channel
  name reads, saved snapshots and command acknowledgements.
- `src/safety.rs`: shared per-packet write gate plus a separate non-switchable
  strictly read-only diagnostic transport.
- `src/document.rs`: TSL/ALB codecs and atomic document saves.
- `data/`: generated factual parameter and UI-control metadata.

The simplified UI intentionally removes the generic raw/system editor. Some
settings formerly accessible there are not exposed in the new editor. Don't
mistake parameter-map coverage for a supported end-user workflow.

## Hardware testing boundary

**Read `../AGENTS.md` before working with a connected amplifier.** Agent testing
is authorized to read only. No GUI Connect, editor-mode setup/cleanup, DT1,
parameter writes, channel selection, preview, commit, mute, reset or restore.
Mock-transport and offline UI tests may exercise those paths with no hardware.

The GUI's read-only permission allows editor-session setup/cleanup; it is not
an absolute no-write diagnostic connection. For authorized diagnostics use:

```sh
./target/release/rusty-boss ports
./target/release/rusty-boss read-reverb --port <explicit-ALSA-port> > reverb-read.json
```

`read-reverb` uses `Session::observe` and `ReadRequestsOnly`. All outgoing frames
must be identity/RQ1. Preserve and inspect its packet audit. See
[PANEL-VALUES.md](PANEL-VALUES.md) for the existing authorized read-only capture
and the distinction between a panel macro position and an algorithm level.

Do not turn a timeout or missing acknowledgement into an automatic write retry.
Cancellation isn't rollback. Hardware state after a failed save can be unknown.

## Offline UI tests

`src/gui/tests.rs` covers actual pointer click/drag events, protection states,
Yes/No save behavior, source/destination snapshots, rapid preset clicks, stale
preview completion, readback/live arming, import failures, every exposed
algorithm/color, model visibility, empty libraries and responsive layouts.
Session tests validate outgoing mock packets and failure ordering. These tests
do not prove audible results or device/firmware interoperability.

For desktop smoke checks, use Xvfb and an idle worker. **Unset `WAYLAND_DISPLAY`
and `WAYLAND_SOCKET`** so winit doesn't choose the real desktop instead of the
isolated X11 display. `RUSTY_BOSS_SMOKE_TEST=1` closes the app after a bounded
number of frames (`RUSTY_BOSS_SMOKE_FRAMES`, default 4). Startup performs no MIDI
port enumeration or connection.

## CLI tools

```sh
rusty-boss ports
rusty-boss schema --filter booster
rusty-boss new my-tones.tsl --name "My tones"
rusty-boss inspect my-tones.tsl
rusty-boss inspect amplifier.alb
rusty-boss request 20000000 16  # Prints a request; never sends it.
```

`new` refuses to overwrite an existing path. The CLI exposes no generic
hardware-write command. Desktop document saves use a same-directory temporary
file, fsync and atomic rename.

## Regenerating metadata

Use a separately extracted, matching original HTML tree; the scripts parse
inert declarations and do not evaluate JavaScript:

```sh
python3 tools/recover_schema.py /absolute/path/to/Resources/html
python3 tools/recover_editor.py /absolute/path/to/Resources/html
```

Keep artifact identities, evidence IDs and limitations in technical docs. Do
not present static recovery or mock tests as observed hardware behavior.
