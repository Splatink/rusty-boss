# Rusty Boss 1.0.0 release checklist

## Status

The project version is **1.0.0** and the current stable Rust/dependency migration
passes local technical validation. **Do not interpret this as unconditional
public-release clearance.** The remaining owner/provenance and hardware-validation
gates below have not been completed. No release tag, upload or publication was
performed by the agent.

## Verified locally on 2026-10-19

- Rust/Cargo 1.99.1, edition 2024; `rust-version` is a minimum, not a ceiling.
- All direct crates at the latest stable crates.io release when checked, and no
  compatible transitive updates remaining. Older parallel transitive versions
  follow upstream semver requirements rather than forced incompatible overrides.
- `bash tools/check.sh`: 98 default, 64 headless, 83 GUI-only and 64 MIDI-only
  tests; formatting, warning-free Clippy for all four configurations and the
  release build.
- RustSec audit: zero known vulnerabilities and zero advisory warnings. This is
  a point-in-time advisory check, not a security audit of the application.
- Release GUI startup/exit under isolated Xvfb for all four themes with temporary
  config/data directories and no MIDI access.
- CLI version, offline TSL creation/inspection and unsent RQ1 packet generation.
- Dependency inventory: all 349 registry archive checksums, extracted manifests
  and 564 archive notice candidates verified. Default Linux normal/build notice
  bundle includes 201 packages and embedded fonts.
- `cargo package --allow-dirty --locked --offline`: packaging and build
  verification of the extracted crate passed. The file list includes notices,
  fonts, assets and locked dependencies; excludes vendor installers, private
  evidence and personal preset files.

Details and retained historical coverage: [VALIDATION.md](VALIDATION.md).

## Required before calling the public release cleared

- [ ] Owner confirms the application license grant/variant and contribution rights.
  The existing GPLv3 text is now referenced by Cargo's `license-file`; this is not
  a newly invented ownership or licensing grant.
- [ ] Resolve the rights/provenance question for vendor-derived parameter/UI JSON.
  See [LICENSE-REVIEW.md](LICENSE-REVIEW.md); version upgrades do not resolve it.
- [ ] Review the final notice bundle. Six egui-family crates had an unavailable
  published VCS revision; their notices explicitly use the 0.37.0 release tag
  instead. Exact-revision verification remains outstanding. Collected filename
  candidates do not prove that every embedded third-party notice was found.
- [ ] Complete user-authorized hardware validation for supported models/workflows,
  especially saved-channel writes, PANEL persistence, system settings and restore,
  or retain explicit experimental limitations. Agent tests must not write to the
  amplifier; mock tests and earlier read-only captures are not substitutes.
- [ ] Manually test native Wayland, clipboard behavior, file chooser portals and
  accessibility on supported desktops. Xvfb tests only exercise X11 startup.
- [ ] Inspect final archives and provide corresponding source/build materials and
  dependency sources/notices as required by applicable licenses. Test installation
  on a clean supported distribution; this host's build is not a portable Linux ABI
  guarantee.

## Build and source-package verification

```sh
bash tools/check.sh
# Install the current stable cargo-audit separately if needed:
cargo audit
cargo package --allow-dirty --locked --offline
./target/release/rusty-boss --version
```

`--allow-dirty` is useful while validating this preparation; a published release
should come from a reviewed clean commit. The source package is
`target/package/rusty-boss-1.0.0.crate`. Do not publish directly from a dirty
workspace or tar the parent workspace containing the vendor installers.

For a Linux binary distribution, stage only the intended executable, README,
CHANGELOG, `LICENSE`, `NOTICE.md`, `THIRD-PARTY-NOTICES.md`, the font attribution,
this checklist and the reviewed source/build materials. The desktop entry and
SVG icon are in `packaging/`. Preserve the notices next to the installed app;
the executable alone is not a complete distribution. Verify the staged archive
and record its checksum after the release gates are satisfied.
