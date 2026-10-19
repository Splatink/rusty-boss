# Changelog

## Unreleased

### Fixed

- Library saves stage only the chosen channel row before export, rather than
  writing each preset through the active PANEL/edit buffer. Bulk saves use the
  same path. Library previews switch from PANEL to A1 before writing tone data.
- Channel clicks switch and read back the amp without creating library entries.
  The editor keeps channel sounds and edits separate from library tones;
  copying to the library remains an explicit action. Read-only clicks explain
  why switching is blocked instead of silently copying.
- Offline regressions cover four-channel saves from PANEL, preview isolation,
  export failures and channel selection with a full library. Hardware persistence
  and audible results remain unverified; no amplifier writes were used in testing.

## 1.0.0 — release preparation

### Updated

- Rust edition 2024 and Rust/Cargo 1.99.1 baseline; newer stable compilers are
  supported without an exact toolchain pin.
- Latest stable direct dependencies checked on 2026-10-19: anyhow 1.0.104,
  serde 1.0.229, serde_json 1.0.154, clap 4.6.7, eframe/egui 0.37.0,
  alsa 0.12.1, rfd 0.17.2 and tempfile 3.27.0.
- Refreshed compatible transitive dependencies and the lockfile.
- Migrated GUI panels, app entry point, tooltips, font access, file drops and
  headless UI tests to current egui APIs. Clipboard and link support remain
  explicitly enabled; X11 and native Wayland remain enabled.
- Version displayed by the CLI and in Settings comes from Cargo metadata.
- Expanded repeatable checks to cover tests and warning-free Clippy in default,
  headless, GUI-only and MIDI-only configurations.
- Updated dependency/license inventory and collected Linux dependency and
  embedded-font notices for distribution.

### Release limits

This version bump does not certify hardware safety or legal clearance. Read
[docs/RELEASE.md](docs/RELEASE.md) before publishing. Hardware support remains
experimental; back up the amp before user-operated writes. The migration did not
change the hardware write-permission policy or the saved library/preset formats.
