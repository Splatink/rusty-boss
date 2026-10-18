# Preset workflow: evidence and independent UI design

## Questions answered in this pass

1. What happens when an original-app library preset is clicked? **Answered
   statically:** a temporary write, local parameter refresh, editor readback and
   preview unmute. No runtime/audible behavior was observed.
2. How does a library-to-channel drag identify its source and destination?
   **Answered at the controller/service boundary:** selected source cells and the
   destination instrument index feed export state, followed by `exportInst`.
   Full lower-level staging/export internals were not newly traced here.
3. How are channel names obtained? **Answered statically:** a dedicated name-read
   workflow updates the channel labels. Rusty Boss reads the known name field of
   each available saved slot without switching channels.
4. How should Rusty Boss simplify that workflow while retaining protection?
   **Independent design, implemented and tested offline**, not a vendor claim.

## Artifact and retained REA evidence

Supplied macOS Tone Studio Gen 3 1.1.0 (26), already extracted at:

`analysis/macos/BOSS TONE STUDIO for KATANA Gen 3.app/Contents/Resources/html`

- HTML tree SHA-256:
  `fcfd680fb9b2bbd9194ea1002e9f70dbacfd27881790aeb372220a0fd0c50181`
- Selected-source analysis parent:
  `ev_3e5b7eb3f9ee5796e95271624a33e562daa3f3afdbb406d744d9f2f62de76ea7`
- Summary:
  `ev_c21946386be15f243b4b02662f0bba8147f318f1c541ca2bccb9fc2087dfef8d`
- Complete local bundle: `evidence/rea-preset-workflow-bundle.json` (git-ignored).

REA parsed these three packaged files as inert JavaScript, with no parse
failures. Direct source-range inspection supplements the conservative graph:

| File below `js/businesslogic/bts/` | Source anchors | Observation |
|---|---|---|
| `librarian/assign_librarian_dynamic_dom_controller.js` | `_patchCellEventHandler`, approximately lines 958–1014 | Click handler sends preview mute, calls `temporaryWrite(currentPatch, 0)`, refreshes temporary parameters, calls `readEditor([0])`, then unmutes. |
| Same file | sortable stop/export branch, approximately lines 506–538 | Captures source `targetPatches` and destination `instPatchIndex`, then invokes `librarianServices.exportInst`; label update follows success. |
| `librarian/librarian_services.js` | `exportInst`, approximately lines 317–420 | Reads target rows through the librarian observer, handles export completion using `sendCommandExport`, then refreshes patch names. |
| `patch_controller.js` | `fetchPatchNames`, approximately lines 274–310 | Reads names through `readEditor([2])` and updates channel labels; this is separate from selecting a channel. |

**Limitations:** 146 inventoried JavaScript files were outside the selected
analysis scope. Static event registrations, dynamic globals, observers and
callbacks do not prove runtime reachability, timing or successful hardware
transactions. The original app was not executed. The bundle's summary is
complete within its view, not complete application coverage. No claim is made
that our save uses the original librarian staging protocol byte-for-byte.

## Rusty Boss's new workflow

The editor now has **Editor**, **Library** and **Settings**. Presets and
model-filtered channel cards share the left rail; sound sections are buttons
above the editor. Raw/Regular switches, the raw block sidebar and the System tab
are removed. Backup/recovery and logs are collapsed in Settings. There is no
system-parameter editor in this UI.

### Preview

- Startup is offline and read-only. Permissions remain explicit and visible.
- With **Preview & edit**, clicking a preset captures it and immediately queues
  a temporary-buffer preview on the MIDI worker. No per-tone confirmation.
- Readback follows preview automatically, as in the observed source workflow.
  The selected tone becomes live only after successful completion for the same
  generation. Existing extra fields and notes are retained.
- Rapid clicks keep only the latest pending preview while an earlier transaction
  finishes. An old completion cannot synchronize/arm a newly selected tone.
- Revoking permission invalidates pending previews and edits. Read-only clicks
  only select the local tone and explain why the amp did not change.
- Viewing pages, themes, algorithms and banks sends no commands and never
  normalizes imported parameter bytes. Changing pages does not require re-arming.

### Save

- **Enable channel writes… → Yes/No** replaces the typed phrase. The per-packet
  transport gate still enforces permissions; hiding UI controls is not protection.
- Dragging captures a **patch snapshot**, not a mutable list index or a reference
  to whatever happens to be in the hardware buffer. A channel drop captures its
  destination too. PANEL/absent channels are rejected.
- The overwrite modal names the tone and destination and offers **Yes / No**.
  No, Escape or dismissal sends no MIDI save job.
- After Yes, the worker checks persistent permission before changing even the
  temporary buffer. `Session::store_patch` validates destination and all writable
  patch data, previews the captured snapshot and performs the existing commit
  sequence. It does not select the destination and accidentally load its old sound.
- If preview fails, no commit follows. No acknowledgements are retried. Success
  updates the card's name; live controls stay linked only if the saved snapshot
  matches the selected local tone.
- Locking during a transaction is checked per outgoing packet. Sent packets
  cannot be recalled; cancellation/locking/error is not rollback. A failed save
  can leave an unknown hardware state. Disconnect/error resets UI permissions.

This deliberately uses the existing validated temporary-buffer/commit primitives
instead of copying the original multi-row library staging/export implementation.
Hardware correctness of the existing commit sequence still needs authorized
model/firmware-specific validation.

### Files and appearance

Native XDG-portal file choosers run off the UI thread. TSL file-drop import is also
supported. Import asks before discarding unsaved local work; reads of the current
amp sound create a new local tone instead of replacing an imported one. Save file
is explicitly different from saving a channel.

Puppet (default), Kill and Lightning are dark. Justice is pearl white with pale
green fills and readable dark-green text accents. Lightning mixes metallic
steel-blue surfaces with purple selection/interaction accents. Puppet carries
warm copper/earth tones through surfaces, controls and interaction states instead
of only changing the accent. The palette includes cards, controls, knobs, text,
selection and effect-bank contrast, not just the accent color. Only appearance is persisted; permissions aren't.

## Verification boundary

Tests exercise actual egui pointer click and drag/drop events using an in-memory
job collector, plus state transitions, mock protocol reads/saves and failures.
Rendering checks cover themes, sizes, pages, model codes, all exposed algorithms
and all color banks. Xvfb smoke runs use an idle, unconnected MIDI worker. No
connected-amp enumeration, GUI connection or amplifier traffic was performed in
this pass. Earlier user reports and the separate authorized read-only reverb
capture remain limited to their original scope.

Native portal-dialog selection itself is not automated by the offline tests.
Audible preview, persistent-save results, mute/export timing and real vendor-file
round trips remain unverified on hardware. See [PARITY.md](PARITY.md).
