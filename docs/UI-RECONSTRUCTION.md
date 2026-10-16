# UI-focused reconstruction pass

## Boundary

The user reports that the initial Rusty Boss implementation works with their
amplifier. That report is encouraging but does not identify the model/firmware
or independently verify every read, write, notification or export path.
The amplifier remained connected during this development pass. **The agent did
not enumerate its ports, connect, request identity, select a channel, preview,
commit, or send any other traffic to it.** All tests used constructed documents,
mock transports and offline egui contexts. Desktop smoke tests used an isolated
Xvfb display and an idle, unconnected MIDI worker. Vendor applications were not
executed; their JavaScript was not evaluated.

## Retained evidence and scope

- UI selected-JavaScript parent:
  `ev_c3a2cc10112fba777054708ca1e4077a70d3919afff981d3a1e6a4531c612df5`
- Summary: `ev_b940d3e90f04ea511d4c1d8f891af21bb458e5b751651ecc9e95bcfd4fe3d1ac`
- macOS HTML tree identity:
  `fcfd680fb9b2bbd9194ea1002e9f70dbacfd27881790aeb372220a0fd0c50181`
- Complete retained records: `evidence/rea-ui-bundle.json` (local, git-ignored).

REA selected six files: `effect_controller.js`, `chain_controller.js`,
`chain_config.js`, `model_controller.js`, `item_logic.js`, `dial_controller.js`
(their full paths are retained in the bundle). Relationships through **143
unselected inventoried files remain unknown**. This is not complete application
analysis. Local inert parsing of `export/item.json` and `export/layout.div`,
and direct inspection of `model_info.js`, supplement that selected-source view.
Exact metadata-source digests are embedded in `data/editor-ui.json`.

## Observations -> implementation

| Observed shipped data/logic | Independent implementation | Limit |
|---|---|---|
| `item.json` PIDs bind widgets to block and offset; layout `description` references labels | 951 numeric UI bindings in `data/editor-ui.json`; validated against schema | Binding count includes alternative presentations, not unique features |
| `frame` and `order` choose an algorithm's page | Standard editor renders only the selected algorithm and color bank | No speculative reset of hidden parameter memory on type changes |
| `list_order` maps option order to nonsequential values | Preserve e.g. booster CLEAN BOOST = 1, MID BOOST = 0 | Unknown imported values remain unchanged |
| `item_logic.js` stringers use finite tables or JS display expressions | Finite text tables and a small explicit Rust format whitelist (ms, Hz, dB, half-dB, seconds, etc.) | No JS eval; unrecognized formats remain numeric |
| Shared stringer has 15 solo-delay high-cut labels, but that widget's max is 10 | Exclude unreachable indices 11..14 from that control | Static mismatch retained as a limitation, not silently “fixed” to a guessed range |
| MOD uses FX/FX_DETAIL banks 1..3; FX uses 4..6 | Independent green/red/yellow selection and bank-aware controls | Editing one bank does not overwrite another |
| DELAY uses DELAY 1..3; DELAY 2 uses 4..6; DELAY 2 shares REVERB's color selection | Separate DELAY 2 page, shared color index | Full reverb-layer/panel-switch coordination not yet ported |
| `effect_controller.js` amp type array: Acoustic, Clean, Pushed, Crunch, Lead, Brown | Amp type dropdown, variation, gain/volume/B/M/T/presence rotary controls | Original artwork and knob assets not reused |
| Selected dependency rules for Guitar Sim, Pitch Shifter, Harmonist, Humanizer, DC-30 | Disable the corresponding inactive controls | Other dependent ranges, assignment rules and label transpositions incomplete |
| `chain_config.js`: six pedal/SR combinations, each with nine patterns, plus four EQ placements | 54 base arrangements / 216 full routing combinations; clickable diagram and selectors | No chain drag/drop, bypass artwork or exact vendor geometry |
| `model_info.js` Bloom on HEAD/Artist/Artist HEAD; solo delay only Artist models; 50 has no GA-FC | Basic capability helpers, Bloom and solo-delay visibility, offline model preview | Not exhaustive capability/assignment/firmware filtering |

## Deliberate safety and presentation choices

These are Rusty Boss policies, **not claims about vendor behavior**:

- Startup does not even enumerate MIDI ports. Refresh and Connect are explicit.
- Permission resets to **Protected / reads only** at every launch.
- A shared atomic gate checks outgoing packets immediately before transport send.
  Identity/RQ1 and editor-mode 0/1 setup/cleanup are allowed while protected.
  Tone parameters, channel selection, saved-channel/system writes and export/reset
  commands are blocked in that mode.
- **Allow temporary edit-buffer changes** permits active-patch data and an explicit
  channel select; it still blocks saved-channel/system and export/reset writes.
  Live editing additionally requires a prior read and explicit arming.
- Saved writes require typing `ALLOW SAVED WRITES`. A commit separately requires
  typing the destination channel label. PANEL cannot be committed.
- If permission is revoked after this transport has muted during a commit, its
  matching unmute cleanup may still pass. It cannot initiate a new mute under
  protection. No cleanup is guaranteed after USB loss, crash or kill.
- Pending edits coalesce per parameter; disarming drops unsent edits. An already
  submitted packet cannot be recalled. Cancellation is **not** rollback.
- Merely viewing pages/algorithms must not clamp or normalize imported bytes.
- Chain clicks navigate instead of toggling bypass or writing routing. Routing
  changes use explicit selectors. All local edits remain local unless live is armed.
- The raw editor remains available as **Advanced raw**. Detailed assignments,
  global/system pages and some capabilities still use this technical view.

## Remaining parity work

This is a usability milestone, not a 1:1 or full-parity release. Next useful
source work: assignment-dependent PIDs/ranges; reverb/delay layer and panel-knob
coordination; complete model filters; system/line-out/global EQ workflows; chain
move validation; and persistent multi-liveset management. Hardware validation
still needs bounded recordings, exact model/firmware and known-good vendor exports.
Bulk hardware restore stays unavailable. See [PARITY.md](PARITY.md).
