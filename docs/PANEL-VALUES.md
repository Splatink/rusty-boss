# Panel positions, effect levels and editor refinement

## Current macro support

The snapshot-only implementation described below has been extended. Dedicated
PANEL macro controls now send only valid TEMP macro values in live preview mode,
then read PATCH back to reconcile firmware-coordinated changes. Watcher DT1s are
retained during reads/acks and update the live panel display; this is not a polling
physical-position meter. The narrow write gate still rejects generic TEMP writes,
out-of-range/sentinel writes, and every macro write in read-only mode. Effect
Level and PANEL position remain different fields with no guessed scaling.

The new paths were tested only on memory transports, never the connected amp.
Hardware/reboot persistence remains unverified. See [PARITY.md](PARITY.md).

## Historical snapshot-only milestone

## Questions and status

| Question | Result |
|---|---|
| Is reverb's physical knob position the same parameter as Effect Level? | **Answered statically: no.** Separate read-only watcher and writable algorithm field. |
| Is there a universal 68-to-50 conversion to apply? | **Not established.** Do not alter the algorithm value or invent a scaling curve. |
| Does this explain the user's specific reading? | **Hardware-supported distinction:** a strictly read-only KATANA-50 snapshot returned panel 52 and active red SPRING Effect Level 68, with PRESET knob assignment. This does not independently measure the physical dial angle. |
| Improve Standard editor presentation | **Implemented:** theme, hierarchy, navigation, cards, ticked rotary controls, responsive rows, compact secondary controls. Not full vendor UI parity. |
| Continuous physical-knob monitoring / macro editing | **Unresolved/unimplemented.** Snapshot presentation only; TEMP writes and coordinated readback are not enabled by this change. |

## Static evidence

REA selected-JavaScript parent:
`ev_21f9769ca97b0e2158f8ac60ab431a0bb393a369a2b63ec74489d89f59521b74`.
Summary: `ev_c5df7b72c91f771220b36ba0f1827938a878316de8ebaab9aec07d234bd07438`.
Tree identity: `fcfd680fb9b2bbd9194ea1002e9f70dbacfd27881790aeb372220a0fd0c50181`.
Complete local records: `evidence/rea-panel-bundle.json` (git-ignored).
Five files selected: `address_map.js`, `assign_setting.js`, `assign_controller.js`,
`effect_controller.js`, `knob_controller.js`; paths are retained in the bundle.
144 other inventoried sources and dynamic relationships remain outside this analysis.
The analysis is inert syntax inspection, not execution, audio observation or proof
of firmware behavior. Direct reading of `export/item.json` supplements it.

All source references below are relative to the macOS extracted `html/` tree:

- `export/item.json:1561–1584`: `panel-reverb-knob` and spinner bind to
  `TEMP%TEMP_KNOB%4`, with `vofs: 1`. The watcher binds instead to
  `PATCH%PATCH_KNOB_READONLY%4` with the same bias.
- `js/config/address_map.js:997–1018`: watcher values range from -1 (unavailable)
  to 100, with wire bias +1. Thus wire 0 means unavailable, 1 means position 0,
  51 means position 50, 101 means position 100. PATCH watcher root offset is
  packed `00011C00` (`20011C00` in the active patch); TEMP knob root is
  `30000200`. These are distinct from the reverb algorithm banks.
- `export/item.json:12111` and `address_map.js:395–403`: Reverb Effect Level is
  `PATCH%REVERB(1)%10`, offset `0A`, bias zero, range 0–100. The three REVERB
  bank roots are `20003400`, `20003600`, `20003800`.
- `js/businesslogic/bts/effect_controller.js:394–401`: panel watcher handler
  propagates the reported value to the TEMP knob/spinner widgets. It does not
  compute position from Effect Level in that handler.
- `js/businesslogic/ktn/assign_setting.js:34–44`: the reverb knob target list
  distinguishes PRESET, reverb time, pre-delay, low/high cut, density, effect
  level and direct mix. A universal conversion from algorithm level to physical
  knob position is therefore not justified by the inspected code.

SHA-256 of directly inspected metadata/sources:

| File | SHA-256 |
|---|---|
| `js/config/address_map.js` | `8fc47cef729de0c8095d4aa771bb39c623c752a1b1cff1e9404ad90447a69f2f` |
| `js/businesslogic/bts/effect_controller.js` | `44666c4b24849ece054459cfa9c45d762c2a4e8077996729ad6f468ce8312098` |
| `js/businesslogic/ktn/assign_setting.js` | `3bba53b462f8f05f39a9a4860fc47853a03318dff0fb1a87a0dd8ba9714d1121` |
| `export/item.json` | `c6be5086917c25f89f86fd7b2fb05118108c0aa47bc47fdb27bc762693dc5c73` |

## Implementation and safety

`Catalog::panel_position` decodes only the dedicated PATCH read-only watcher.
It does not fall back to Effect Level, derive a curve, or coerce unavailable or
out-of-range values to zero. Effect cards display this as **PANEL n / 100**;
a dash means no valid snapshot position. The Reverb editor explicitly separates
it from Effect Level. Imported snapshot data can also contain these fields, so
readouts are not presented as verified current hardware telemetry.

The existing GUI patch-read path already includes these blocks. No additional
GUI requests, polling or writes were added. The control is deliberately not editable:
writing a TEMP macro may alter several algorithm parameters, which would require
coordinated readback and a separate safety review. Editing an algorithm level
leaves the last-read position unchanged until another patch read; it is a snapshot,
not a live mirror. The initial UI work was offline; after the user explicitly
authorized hardware reads (never writes), the separate diagnostic below was used.

The visual design is independent Rusty Boss work, not copied vendor artwork:
slate surfaces, warm accents, consistent spacing/type, full-width sidebar buttons,
G/R/Y bank chips, active-effect borders, ticked rotary controls with direct entry,
collapsible secondary amp controls and separate details/signal-path cards.
Editor-mode write gating, live edit queue, preset protection and read sequencing
were not changed.

## Verification and remaining work

Constructed fixtures cover panel 50 with Effect Level 68, +1 bias endpoints,
unavailable/missing/short/invalid data, independence across algorithm banks,
and read-only watcher policy. GUI tests render all sections at 800×560,
1180×800 and 1600×1000 without altering unknown telemetry bytes or queuing writes.
Existing all-algorithm/bank inert-render tests remain in place. Screenshot smoke
checks use only Rusty Boss, offline, on isolated Xvfb; original JS was not executed.

To validate the reported discrepancy, read the current sound and compare the
Reverb card's **PANEL** value with **Effect Level**. A physical position of 50
alongside an Effect Level of 68 is now represented as two separate fields, not
silently normalized. If PANEL itself is wrong, retain the two raw blocks plus
model/firmware, selected color and knob assignment for the next investigation.
The capture below confirms that the device reports separate position/level values;
there is no justification for overwriting 68 with a guessed rescaled value.

## Authorized hardware observation — reads only

After the static/UI work, the user explicitly authorized amp testing with an
absolute **no writes** restriction. The normal GUI connection was not used:
its protected mode still sends editor-mode setup/cleanup DT1s. Instead,
`read-reverb --port hw:3,0,0` used `Session::observe` over `ReadRequestsOnly`.
This transport has no permission toggle or cleanup exception; it rejects every
DT1 and other non-identity/non-RQ1 frame before forwarding to ALSA.

Observed report: `tests/fixtures/katana50-reverb-readonly.json`, SHA-256
`e04671ce0f53c1e2bb631ba2dec2b1c63a1cf619380a004cab9d189c4ac4ccc0`.
This is an application-recorded MIDI exchange, not an independent USB capture
or REA observation of the original application.

- Identity decoded as KATANA-50, model code 05, device ID 10 hex.
- Selected reverb color red; algorithm SPRING; Effect Level **68**.
- Read-only panel watcher **52** (wire byte **53 decimal**).
- Reverb knob assignment **0 / PRESET**.
- Green SPRING Effect Level 65; yellow HALL Effect Level 72.
- Color and panel-watcher bookends matched. Reads are sequential, not atomic.
- Exactly **one identity request and eight RQ1 reads**, nine received frames,
  no omitted frames, **zero outgoing DT1 writes**. No editor-mode command,
  channel select, mute, preview, commit, reset or cleanup write was sent.
- Firmware version and physical dial angle were not independently interpreted
  or measured. The observed 52 is not asserted to equal the user's earlier 50.

The retained-fixture test checks the actual red-bank binding, panel bias, level,
assignment, all recorded outgoing command types and incoming Roland checksums.
The generic constructed test still covers the user's illustrative 50/68 case.
The diagnostic performs at most eight bounded block reads, never retries, emits
partial/error reports without default-filled values, and needs an explicit port.


## PANEL mode versus the PANEL macro readouts

These are distinct concepts. **PANEL mode** is the amplifier's slot 0; the
**PANEL** numbers on effect cards are last-read macro positions. The latter do
not constitute a complete PANEL editor.

Rusty Boss includes PANEL (slot 0) in the channel rail for selection and
confirmed drag-to-save. Persistent permission is required before the worker
stages the captured tone directly in the PANEL row and exports destination 0.
A/B saves no longer preview through the active buffer, and library previews
leave PANEL for A1 before sending tone data. Channel selection is separate from
explicit library copying. See [PRESET-WORKFLOW.md](PRESET-WORKFLOW.md) for the
corrected save path and its offline regression coverage.
No hardware writes were used to validate this workflow.

The earlier decision to reject PANEL saves is a Rusty Boss restriction, **not a
claim that the hardware cannot write PANEL**. The user reports that saving a
PANEL effect assignment (e.g. green MOD = Pitch Shifter) survives reboot, whereas
software knob settings are replaced by the physical knobs on power-up. This
specific persistence distinction has not been tested independently.

Static corroboration: `js/businesslogic/bts/patch_controller.js:359–390` in the
supplied original builds `PATCH_WRITE` with `00 + hex2(value)`, then calls
`sendCommandExport(false, true, true, value, ...)`; the handler has no destination-0
exclusion. See the selected-source REA parent
`ev_3e5b7eb3f9ee5796e95271624a33e562daa3f3afdbb406d744d9f2f62de76ea7`
and [PRESET-WORKFLOW.md](PRESET-WORKFLOW.md). This corroborates a write route; it
does not prove which fields firmware restores after reboot. The destination-0
restriction has now been removed; the persistent permission gate, model/slot
bounds, full-patch preflight and overwrite confirmation remain in place.

## Knob, routing and PANEL workflow follow-up

Selected-source REA parent:
`ev_98473656e6d5eb7e0d98bb735aab859ba83f0ec78bb35d55ea44f70e3a9475fd`.
The same HTML tree identity above was retained. Three sources were parsed inertly:
`chain_controller.js`, `patch_controller.js`, and `model_info.js`; 146 other
inventoried sources and dynamic relationships remain outside this analysis.
Direct source inspection additionally checked `ChainEffector.js`, `ChainFV.js`
and `model_controller.js`; none was executed.

- `chain_controller.js:67–93` resolves a moved block back to base-pattern,
  pedal/SR and EQ-position parameters. `ChainEffector.js:73–80` inserts the
  block **after** the destination. Rusty Boss accepts only exact supported
  single-block moves; it never guesses an arbitrary firmware chain layout.
- `chain_controller.js:478–488` fixes AMP, DELAY 2 and REVERB in place;
  `ChainFV.js:6` also disables volume movement. IN/OUT remain fixed.
- `model_info.js:29–30, 62–67` includes PANEL in existing patches;
  `model_controller.js:201–208` shows existing patches in the WRITE selector.
  Combined with the write handler above, this supports destination 0 in the
  app workflow, without proving device persistence or audible behavior.
- Offline input tests cover immediate post-click arrows on dial/numeric controls,
  repeated steps, range limits, disabled input and blur without phantom sends.
  An additional headless diagram drag checks local routing and queued edits.
  Mock tests now cover confirmed PANEL saves using destination-row staging and
  destination-0 export rather than preview-then-PATCH_WRITE.

No amplifier connection, requests or writes were used during this follow-up.
TEMP macro editing, continuous physical-knob telemetry and hardware/reboot
verification remain out of scope. A saved PANEL tone includes all writable
patch fields; it is not an effects-only merge with the previous PANEL settings.
