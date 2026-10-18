# Feature parity and remaining work

**This is not a completed full-functionality replacement.** A recovered address
map is not application parity, and a passing mock transaction test is not hardware
verification. This document tracks technical coverage and remaining work, not a goal to copy
Tone Studio one-to-one. The user guide is [README.md](../README.md).

| Area / investigation question | Evidence status | Rusty Boss implementation | Verification / remaining work |
|---|---|---|---|
| Identify both supplied releases | Answered statically | Schema identifies 1.1.0 (26) | Installer/app hashes in investigation |
| Find packaged editor/protocol | Answered for selected JS files | Independent Rust codec + factual generated schema | No native wrapper execution; not all 149 relevant files semantically analyzed |
| Compare platforms | Answered within HTML subtree | Shared schema used | Does not prove native wrapper/platform equivalence |
| Parameter address/encoding/ranges/defaults | Answered for shipped map | 106 blocks / 2,463 instances; bounds/encoding tests | New firmware and undocumented addresses unknown |
| Amp/effect/EQ/solo/contour/pedal/SR controls | Selected UI workflows recovered statically | Single amp/effect editor, algorithm-specific pages and color banks; 951 numeric UI bindings; no raw fallback | Some recovered units and dependencies; complete coordinated-control/type-change semantics remain incomplete |
| System, USB, line-out, MIDI, global EQ | Partially answered | System backup reads retained; generic system editing removed from the simplified UI | No automatic persistence or bulk system restore; hardware untested |
| Model differentiation | Static identity/capabilities found | Codes 05..0B, channel masks; Bloom and solo-delay visibility; offline model preview | Power/GA-FC and exhaustive capability filtering incomplete |
| Color-bank changes / assignments | Selected bank/page bindings recovered | Friendly green/red/yellow banks, independent MOD/FX and shared DELAY2/REVERB color; basic knob/footswitch selectors | Full layer/panel coordination, dynamic assignment PIDs/ranges and pedal calibration incomplete |
| Chain/routing | Original base/EQ tables statically recovered | Nine patterns, position selectors and clickable ordered diagram; 54 base / 216 complete combinations tested | No drag/reorder interaction or exact vendor geometry/bypass artwork |
| Connection | Static handshake recovered | ALSA discovery + identity + level + mode | Needs actual Gen 3, supported kernel and device ports; no BLE backend |
| Live editing | Static DT1 observed in source | Successful preview/readback or current-sound copy automatically links parameter controls when permitted; idle DT1 updates | Device notifications during other transactions can be consumed; no exhaustive event reconciliation |
| PANEL editing / persistent PANEL settings | Original write/export handler accepts destination 0 in static code; reboot persistence described by user, not independently tested | Can read PANEL and edit the current sound after selecting PANEL on the amp; no PANEL selector/save target or editable TEMP macro controls | Missing workflow; slot-0 write rejection is our restriction, not a device limitation. Separate persisted effect assignments from physical-knob-controlled values and verify safely. |
| Channel selection | Static command recovered | Send select, confirm current number, then read buffer | Timing/hardware behavior unverified |
| Preview | Static temporary-write logic found | Click-to-preview validated patch snapshot, automatic readback and generation-safe live linking; latest pending click wins | Read-only stored knob blocks intentionally omitted; audible parity unverified |
| Persistent write | Static write/export sequence found | Per-packet permission gate, saved writes locked by default; Yes/No unlock, drag-to-channel with snapshot + overwrite confirmation; model/slot guards | Acknowledgement semantics, mute/export timing and failure cleanup still require hardware tests |
| Local librarian | Static file model found | One in-memory liveset, notes, rename/duplicate/delete/reorder; explicit TSL save/load | Library-to-channel drag/drop and native TSL file chooser/file-drop import available; multi-liveset storage/search, multiselect and bulk transfer incomplete |
| Gen 3 TSL interchange | Static JSON model recovered | Hex-string byte arrays, metadata/unknown fields, default-tail fill | Tests use constructed documents; real exported-file/original-app round trips still required |
| MkI/MkII TSL conversion | Converter files located | Explicit rejection instead of guessed conversion | Conversion tables and regression fixtures need reconstruction |
| ALB backup | Static format/read model recovered | File codec, patch extraction, experimental sequential hardware read | No atomic snapshot guarantee; actual vendor restore validation outstanding |
| All-data hardware restore | Partially traced | **Unavailable** | Must trace export/staging/mute sequence and verify safely before implementation |
| Tone Exchange | Packaged URL configuration observed | Open official page in external browser; local Gen 3 TSL import | Not integrated authentication/upload/download or guaranteed service compatibility |
| Tone Central / tips / log services | Endpoints located, not contacted | Not implemented | Need declared runtime/network evidence and current service behavior |
| Vendor UI, localization, onboarding/help | Selected UI metadata/controllers analyzed | Independent focused Editor/Library/Settings UI, four persistent themes, user guide; no vendor assets at runtime | Not a pixel clone; localization, tips, exact layout and full interaction parity incomplete |
| Linux audio | Not established by editor sources | No DSP/audio engine | Tone Studio controls hardware DSP; audio routing/kernel support is separate |
| Hardware regression testing | **User-reported initial success; not independently verified** | Mock transport, codec/data/UI tests; UI-focused pass used no attached device | Needs exact model/firmware, vendor TSL/ALB fixtures and bounded USB-MIDI recordings |

See [PRESET-WORKFLOW.md](PRESET-WORKFLOW.md) for the current workflow, evidence,
safety policy and test boundary. [UI-RECONSTRUCTION.md](UI-RECONSTRUCTION.md)
records the earlier editor milestone.

## Suggested next milestones

1. Obtain a sacrificial/fully backed-up Gen 3 test amplifier and record its model,
   firmware and Linux kernel/ALSA port identity. Begin with identity and read-only
   comparisons against original Tone Studio exports; no writes.
2. Verify every requested byte of a current patch, model-specific slot layout,
   system settings and ALB rows. Add sanitized fixtures and regression tests.
3. Compare single-parameter writes/notifications and type/color changes; add
   capability gating and human-unit display mappings.
4. Validate preview -> readback -> persistent commit, including timeout, USB loss,
   mute/unmute and reconnect. Do not infer rollback from a missing acknowledgement.
5. Trace and implement bulk restore/export staging; verify each channel independently.
6. Port legacy converters and higher-level chain/assignment workflows with fixtures.
7. Add local multi-library persistence, desktop usability/accessibility refinements,
   packaging and optional service integrations. Do not hard-code private credentials
   or turn recorded endpoint strings into claims of current service availability.
