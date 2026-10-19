# Feature coverage and verification boundary

Rusty Boss aims for the useful non-marketplace workflows, **not** the vendor's
complexity or a pixel clone. The user explicitly excluded Tone Central/Exchange,
legacy preset conversion, localization and manuals. These are not backlog items.
A recovered parameter count or passing mock test is not hardware parity.

## Current implementation

| Area | Available now | Remaining boundary |
|---|---|---|
| Tone editor | Amp/effect algorithms, EQ/EQ2, solo, contour, pedal FX, send/return, color banks; 1,243 recovered UI bindings | Bindings include alternative/dynamic presentations, not unique features. Not every vendor conditional visibility rule is reproduced. |
| Routing | Nine patterns, constrained diagram moves and destination menus; 216 complete combinations tested | Unsupported routes are rejected, never approximated. |
| Panel macros / tap / layers | Dedicated bounded TEMP macro commands with coordinated patch readback; live PATCH watcher notifications; delay/Delay 2/solo-delay taps; per-color Delay 2/Reverb layer selector | No invented physical-knob scaling. Macro edits need a valid live snapshot; notifications are not continuous polling. Hardware behavior is unverified. |
| System settings | One collapsed Amp settings section: global EQ banks/type/position, line-out Air Feel/custom mic/ambience, Power Amp In, model-dependent power control, USB levels/loopback, MIDI PC/CC/channel, system GA-FC | Read first, stage locally, explicitly confirm Apply. Only changed values are sent, after checking for external changes. Reads/writes are sequential, not atomic. |
| Assignments | Algorithm-dependent knob targets; expression function/target/heel/toe; amp tip/ring, GA-FC/EX and expanded GA-FC/EX | 34 literal target tables recovered. Inverted ranges are supported. Controller/algorithm changes preserve inactive settings; target changes reset the active range. Model-specific hardware behavior remains unverified. |
| Connection / live changes | Explicit ALSA connect; identity/slot masks; preview/readback; single-parameter edits; retained unsolicited updates during reads/acks | Native wrappers and firmware timing were not executed/validated. Notification backlogs are bounded; overflow fails rather than silently dropping updates. |
| Channel/PANEL saves | Snapshot capture, permission gate and overwrite confirmation; full bulk preflight; ordered multi-channel saves | Earlier successful writes are not rolled back on later failure/cancellation. Reboot persistence and audible results remain unverified. |
| Local collections | Multiple collections, automatic local saves, previous-file copy, cross-instance revision checks, multiselect copy/move/delete, selected channel reads and writes | One bounded 16 MiB JSON library; up to 100 collections and 1,000 tones per collection. Corrupt/changed files are not silently overwritten. Latest edits before autosave can be lost on a crash. |
| TSL files | Gen 3 import adds a collection; native chooser/file drop; explicit current-collection export; metadata/unknown-field retention | Real vendor-app round trips still need fixtures. Legacy conversion is intentionally excluded. |
| ALB backup / restore | Sequential reads, file codec, patch extraction, exact-model full restore: validated system/slot staging, 10-second wait, one acknowledged all-data export, restore channel selection | Experimental and hardware-unverified. Incomplete/wrong-model backups are rejected before any write. Cancellation/failure can leave a partial restore. Do not use as your only backup/restore tool. |
| Custom icons | Local, bounded PNG imports per algorithm, rendered on cards and chain; reset to defaults | Original/vendor artwork is not shipped. Broken local images fall back to text. |
| Appearance / navigation | Editor, Library, Settings; four themes; advanced settings and transfers disclosed only when requested | No separate raw editor, marketplace, onboarding stack, manuals or language system. |

## New static evidence

The supplied macOS 1.1.0 (26) HTML tree retains identity
`fcfd680fb9b2bbd9194ea1002e9f70dbacfd27881790aeb372220a0fd0c50181`.
New REA parent:
`ev_b647de37786d6578ee457946a188ac43e6fa1c5866eccfd96cd57ea9cd76bcbb`.
Summary: `ev_700180283852d5251afe2f21564d0722283a202e7d81fb8b0d59c8bcf2dc88a1`.
Complete local records: `evidence/rea-feature-implementation-bundle.json`
(git-ignored); summary: `analysis/parity-implementation-rea-summary.json` in the
parent workspace. Nine selected JS files parsed; 140 other inventoried sources and dynamic/global
relationships remain outside this analysis. No vendor code was executed.

Key source anchors under the extracted `html/` tree:

- `js/businesslogic/ktn/assign_setting.js`: literal target lists/knob offsets.
  `assign_controller.js`: dynamic algorithm targets, heel/toe bias/format and
  expanded-controller remapping. `tools/recover_editor.py` parses inert facts;
  no JS evaluation or vendor runtime is included.
- `js/businesslogic/bts/effect_controller.js`: TAP command addresses, TEMP/PATCH
  watcher coupling and color/layer wiring; `export/item.json` supplies bindings.
- `js/businesslogic/bts/backup/all_data.js`: system/stored-row staging;
  `all_backup.js` restore completion: 10-second settling, all-data export, final
  selection. `patch_controller.js::sendCommandExport` defines mute + `7F 7F`
  export and acknowledgement matching.

Independent design choices: explicit staged Apply, optimistic setting conflict
checks, strict complete/exact-model restore preflight, no automatic write retries,
local atomic autosave with previous copy, and confirmed ordered single-channel
commits for bulk tone saves instead of duplicating vendor librarian machinery.

## Verification

Offline tests cover target tables/ranges (including signed and expanded bindings),
system Apply/conflict checks, macro/tap packet bounds, full restore root/order/
export preflight, bulk validation/partial failures, notification retention,
collection restart/corruption/concurrency handling, icon validation, GUI state
and rendering. See [VALIDATION.md](VALIDATION.md) for commands/results.

**No amplifier traffic was sent in this implementation pass.** All new hardware
write paths were exercised only with memory transports. The standing agent test
policy remains absolute read-only, including no editor-mode setup/cleanup. The
old KATANA-50 reverb fixture verifies only its narrow historical read slice.

Next work is validation and refinement, not more feature categories: real Gen 3
TSL/ALB fixtures; model/firmware-specific read comparisons; desktop portal and
usability/accessibility checks. Hardware writes require a separate explicit
change to the user's read-only testing authorization.
