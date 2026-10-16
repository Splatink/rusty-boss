# Local validation results

Validation was run in the supplied Linux workspace, with Rust 1.99.0, ALSA
1.2.16.1 and the locked Cargo dependency set. This records testing of Rusty Boss,
not execution/behavioral equivalence of the original application.

| Check | Result |
|---|---|
| `cargo fmt --check` (after formatting) | Passed |
| `cargo test --locked` | **38 passed**: 28 unit/UI + 10 integration tests |
| `cargo test --locked --no-default-features` | **35 passed**: 25 unit + 10 integration tests |
| `cargo clippy --all-targets -- -D warnings` | Passed |
| `cargo clippy --no-default-features --features gui --all-targets -- -D warnings` | Passed |
| `cargo build --locked` | Passed |
| `cargo build --release --locked` | Passed; native executable about 14 MiB |
| Debug GUI four-frame startup/exit smoke | Exit 0 |
| Release GUI startup/exit smoke | Exit 0; UI continuation also used isolated Xvfb, 80 frames and local MOD navigation, no port enumeration/connection |
| CLI `new` + `inspect` constructed Gen 3 TSL | Passed |
| CLI `request 20000000 16` | Correct expected RQ1 bytes; not transmitted |
| CLI `ports` (initial milestone only) | No duplex raw MIDI ports then; not repeated with the user\'s now-connected amp |
| Original app execution, real TSL/ALB interoperability, USB-MIDI/audio/device writes | **Not independently tested**; user reports initial implementation works, without a captured model/firmware/traffic regression fixture |

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
- Preview preflight validates all fields before any write; direct command-area edits,
  PANEL commits and nonexistent channel commits are rejected.
- TSL and ALB constructed-file round trips, unknown-field preservation, hex bytes,
  null backup rows, reported default-tail fill, product/revision/size rejection.
- Headless egui rendering of every patch/system block, empty library and auxiliary
  pages; startup is offline. A real interactive usability/accessibility review is
  still required.
- Standard editor renders every recovered MOD/FX/delay/reverb/booster/EQ/pedal
  algorithm across color banks without changing a single parameter byte or
  queuing a write. Hidden bank/algorithm parameters remain intact.
- All 951 numeric bindings resolve, reachable finite UI options encode and
  round-trip without touching neighboring bytes; formatted numeric entry inverts
  the known unit rules.
- All 216 chain + pedal/SR/EQ-position combinations have the same unique nodes.
- Preset-protection gate rejects tone/slot/system/command writes while protected;
  temporary mode still rejects stored and export/reset writes. Mock sink proves
  rejected packets never reach transport; permission revocation still permits
  cleanup of a mute sent by that transport.
- Pending edits coalesce by parameter; disarming drops the unsent queue.

The golden fixtures are derived from static source observations, not captured
hardware traffic. The mock protocol tests do not prove the amplifier accepts or
acknowledges these requests. The GUI smoke test proves startup/render/normal exit,
not all interactions. Sequential backup reads are not an atomic snapshot.

Command logs remain in the parent workspace's `analysis/` directory. Detailed
static evidence is in `evidence/` and `docs/INVESTIGATION.md`.
