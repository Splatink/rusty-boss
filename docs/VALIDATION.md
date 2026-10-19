# Local validation results

Validation was run in the supplied Linux workspace, with Rust 1.99.0, ALSA
1.2.16.1 and the locked Cargo dependency set. This records testing of Rusty Boss,
not execution/behavioral equivalence of the original application.

| Check | Result |
|---|---|
| `cargo fmt --check` (after formatting) | Passed |
| `cargo test --locked --offline` | **98 passed**: 84 unit/UI + 14 integration tests |
| `cargo test --locked --offline --no-default-features` | **64 passed**: 50 unit + 14 integration tests |
| `cargo test --locked --offline --no-default-features --features gui` | **83 passed**: 69 unit/UI + 14 integration tests |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo clippy --no-default-features --features gui --all-targets -- -D warnings` | Passed |
| `cargo build --locked` | Passed |
| `cargo build --release --locked` | Passed; native executable about 18 MiB (with native portal file chooser) |
| Debug GUI four-frame startup/exit smoke | Exit 0 |
| Release GUI startup/exit smoke | Exit 0 for all four themes, isolated Xvfb and temporary appearance config, no port enumeration/connection; Puppet screenshot retained |
| CLI `new` + `inspect` constructed Gen 3 TSL | Passed |
| CLI `request 20000000 16` | Correct expected RQ1 bytes; not transmitted |
| CLI `ports` (initial milestone only) | No duplex raw MIDI ports then; not repeated with the user\'s now-connected amp |
| Strict read-only USB-MIDI reverb probe | Passed on KATANA-50 after explicit user authorization: panel 52, active red SPRING level 68, PRESET assignment; 1 identity + 8 RQ1, **0 DT1 writes**. Retained fixture in `tests/fixtures/` |
| Original app execution, real TSL/ALB interoperability, audio/device writes | **Not independently tested**; user reports live buffer and preset loading work. All agent hardware testing is now restricted to reads, never writes |

Run the repeatable local checks with `bash tools/check.sh`. To include the desktop
smoke test on a display-enabled host, use
`RUSTY_BOSS_CHECK_DESKTOP=1 bash tools/check.sh`.

## Test coverage

- Roland checksum/framing and golden DT1/RQ1 byte vectors.
- Base-128 address carry, maximum-address overflow and invalid packed bytes.
- Every recovered numeric parameter at min/default/max, signed biases and nibble
  encoding; out-of-range writes rejected.
- Truncated/corrupt/wrong-device responses, split/reordered/duplicate data,
  conflicting duplicate bytes, incomplete-read failure without retries.
- Identity model codes and channel-presence bounds; handshake/disable-mode behavior
  through a mock transport.
- Preview preflight validates all fields before any write; direct command-area edits
  and nonexistent channel commits are rejected. PANEL saves are covered by mock
  snapshot/write/export and permission/confirmation tests.
- Rotary dial and numeric arrow-key steps, focus retention, bounds and unchanged
  focus-loss behavior have headless input tests. Chain moves resolve only to exact
  supported routes; a headless diagram drag verifies routing and queued edits.
- TSL and ALB constructed-file round trips, unknown-field preservation, hex bytes,
  null backup rows, reported default-tail fill, product/revision/size rejection.
- Headless egui rendering of Editor/Library/Settings, empty libraries, all four
  themes and auxiliary dialogs; startup is offline. Raw/System top-level tabs stay
  removed; staged amp settings render inside Settings. A real accessibility review
  is still required.
- Actual egui pointer click and drag/drop events feed a fake MIDI job collector:
  click previews, drop captures a patch/destination snapshot, No/Escape sends
  nothing, Yes saves the captured snapshot without typed input.
- Read-only/temporary/persistent permissions, latest pending preset wins,
  stale preview completion cannot arm controls, reads preserve downloaded tones,
  and busy/disconnect/error/lock prevent unsafe save submission.
- Model-filtered channel-name reads emit only mock RQ1s. Store validates before
  writing, previews the captured tone before one commit, never selects the
  destination, and never commits after a preview failure or retries missing acks.
- Large livesets keep preset rows and channel drop targets visible simultaneously.
  Theme round trips/fallback and light/dark contrast are checked. Native portal
  dialog selection itself isn't automated.
- Palette regression tests cover Justice's pale green fills (including a switch
  from Lightning with no leftover purple), Lightning's distinct blue and purple
  roles, and Puppet's warm surfaces/controls. WCAG-style contrast checks require
  at least 4.5:1 for body text, accent text on cards and selection ink/fill pairs.
- Header uses bundled Russo One through antialiased font rendering, replacing the
  hard-edged, hand-triangulated spiked wordmark. Live DPI changes at 1×, 1.5×,
  2×, 3× and back to 1× are tested in the same context: glyph atlas texel sizes
  grow with DPI while logical dimensions remain stable. Body font metrics/styles
  remain unchanged. All themes and constrained widths stay on one line.
- Native screenshot checks cover every theme at 1×, Puppet/Justice at 2×, and
  Puppet at 3×. The 2× run used a 3840×2160 Xvfb display; 3× used 3840×2560.
  Captures are native rendering, not resized 1× images. The header-only 2× crop
  is `docs/screenshots/wordmark-2x.png`. No amplifier traffic was sent.
- Standard editor renders every recovered MOD/FX/delay/reverb/booster/EQ/pedal
  algorithm across color banks without changing a single parameter byte or
  queuing a write. Hidden bank/algorithm parameters remain intact.
- All 1,243 numeric bindings resolve, reachable finite UI options encode and
  round-trip without touching neighboring bytes; formatted numeric entry inverts
  the known unit rules.
- Strict observation transport rejects all DT1 (including editor setup/cleanup),
  program/control changes and unknown packets before the sink. Probe mock verifies
  exactly one identity and eight RQ1s, with no implicit cleanup write.
- Real KATANA-50 read-only fixture: selected red SPRING, panel 52, level 68 and
  PRESET assignment; recorded outgoing packet types and incoming checksums checked.
- Panel-position regression: separate macro 50 / algorithm level 68, +1 wire bias,
  unavailable sentinel, invalid/missing/short data and read-only watcher policy.
- All editor sections also render at 800×560, 1180×800 and 1600×1000 without
  modifying unknown panel bytes or enqueueing writes. These are render tests, not
  full automated layout/usability assertions. Offline desktop screenshots were
  checked at 1180×800; an overflowing centered-knob layout found there was corrected.
- All 216 chain + pedal/SR/EQ-position combinations have the same unique nodes.
- Preset-protection gate rejects tone/slot/system/command writes while protected;
  temporary mode still rejects stored and export/reset writes. Mock sink proves
  rejected packets never reach transport; permission revocation still permits
  cleanup of a mute sent by that transport.
- Pending edits coalesce by parameter; disarming drops the unsent queue.
- All 34 dynamic assignment tables resolve and their target/range endpoints encode.
  Expression bindings follow algorithm/color/controller/expansion, including signed
  targets, millisecond targets and reversed ranges; detailed pages render inertly.
- System Apply validates all changes before writing and checks old values for
  external conflicts. Bulk saves preflight every tone and reject duplicate slots.
- Full restore mock checks cover complete exact-model preflight, all stored roots,
  settling interval, one all-data export and final selection. Staging failure,
  settling cancellation and missing export ack never retry/select; failed export
  attempts nonpersistent unmute cleanup. No hardware rollback is promised.
- The preview gate accepts only bounded one-byte macros and TAP presses, not
  generic TEMP/command writes. Reads/acks retain unsolicited updates. Multi-block
  notifications and coordinated readbacks preserve unsent edits and future fields.
- Collection restart/previous-copy, filesystem locking/revision conflicts, corrupt
  files, collection transfer validation and pretty-JSON size limits are tested.
  Oversized exports cannot overwrite a valid file or create an unreadable export.
- PNG type/dimensions/size and icon-key paths are validated; custom icons are copied
  locally without changing tones or queuing MIDI. Native portal choice isn't automated.

Protocol golden fixtures are derived from static source observations. The separate
KATANA-50 reverb fixture is captured hardware traffic from our strict read-only
probe; it establishes only that focused read slice. Other mock protocol tests do
not prove the amplifier accepts or acknowledges their requests. The GUI smoke test proves startup/render/normal exit,
not all interactions. Sequential backup reads are not an atomic snapshot.

This usability pass sent no traffic to the connected amplifier. The earlier
read-only reverb capture is separate. New logs use the `analysis/usability-*`
prefix; the updated palette checks use `analysis/palette-final-*` and refreshed
all four screenshots. Current typography/DPI checks use `analysis/brand-*` (earlier
wordmark checks used `analysis/wordmark-*`). Release screenshots are in
`docs/screenshots/{puppet,justice,kill,lightning}.png`.

The feature implementation checks use `analysis/parity-implementation-*` in the
parent workspace, including an offline native screenshot and retained REA summary.
Metadata recovery was re-run in memory and exactly reproduces the shipped controls,
assignment tables, chains, selectors, groups and source digests. Full source records
are in `evidence/rea-feature-implementation-bundle.json` (git-ignored).

**No amplifier traffic was sent for this feature implementation.** New write paths
were tested only with memory transports. Real Gen 3 TSL/ALB round trips, firmware
write/notification timing, audible equivalence and reboot persistence remain
unverified. The agent's strict read-only hardware-testing restriction is unchanged.
The detailed current coverage/scope is in [PARITY.md](PARITY.md).
