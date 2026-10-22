# Experimental KATANA-AIR support

This branch includes experimental support for the original BOSS KATANA-AIR.
The Air uses its own protocol and parameter map. Its editor exposes Amplifier,
Booster, Mod, FX, Delay 1, Delay 2, Reverb, Equalizer, Noise gate, Tone level/key,
and Cabinet/global EQ sections. Effect controls follow the selected algorithm.
Gen 3 channel addresses and settings must never be sent to the Air.

The device has six saved tones (A and B, each with green/red/yellow selections)
and PANEL. The interface shows that channel arrangement. Channel recall uses
the source-backed experimental program mapping `0, 1, 2, 3, 5, 6, 7`; the two
unused legacy slots are excluded. The app re-reads and verifies the channel
number, then refreshes the live tone. Recall requires Preview permission.
Stored-tone writing, import/export, backup/restore, chain rearrangement,
colour-slot assignments and tuner are unavailable. Live edits are lost on a
channel change unless saved on the amplifier.

## Connection and permissions

Select KATANA-AIR in the header. Pair **KATANA-AIR MIDI** through a Linux
Bluetooth MIDI service that exports an ALSA sequencer port. In Settings, refresh
Air MIDI ports, select the matching `seq:client:port`, then Connect Air. The new
sequencer transport handles MIDI events; Linux handles BLE timestamps and pairing.
The USB/raw-MIDI connection for Gen 3 remains separate. Rusty Boss does not pair
Bluetooth devices itself. `rusty-boss ports --air` lists sequencer ports without
opening an amplifier session.
No editor-mode setup or cleanup writes are sent during Air connection.

Connection checks an Air identity reply (Roland family `50 00`, device `00`).
Air SysEx uses the four-byte model ID `00 00 00 50`, device `00`, Roland checksum,
and base-128 addresses. Refresh reads the temporary tone at `60 00 00 00` for
2,432 bytes and cabinet/global EQ at `00 00 04 30` for 15 bytes. Requests are
split into chunks of at most 128 bytes. Every requested tone byte must arrive before
a snapshot is published; incomplete tone refreshes disable editing. The global
block and current channel at `00 01 00 00` (two bytes) are optional: if they do
not arrive, the snapshot contains a warning and their controls remain unavailable.
Old optional values are never reused.

Protected mode permits identity and parameter reads. Preview permission permits
edits to the temporary Air tone. Cabinet/global EQ requires persistent-write
permission because those settings apply across tones. The shared permission
gate is checked immediately before each outgoing edit. Each edit re-reads the
current value, refuses unknown or out-of-range values, writes the setting, then
verifies it by readback. A failed edit or verification invalidates the snapshot;
Refresh is required before further editing. Failed verification is reported and
does not trigger an automatic restoration write.

## Evidence and remaining limits

The implementation and bundled metadata use the public GPL-3.0-or-later
[KatanaAirDesktop source](https://github.com/npsfsz/KatanaAirDesktop/tree/a442bffe4e09941be0c75265993932ee9ac19f63),
commit `a442bffe4e09941be0c75265993932ee9ac19f63`:

- `source/protocol.py`: framing, identity family, current-tone and global reads,
  corrected preamp area, and edit readback.
- `source/metadata.json`: parameter addresses, option labels, and effect layouts.
- `source/index.html`: algorithm-dependent control selection and value displays.
- `README.md` and `NOTICE`: scope, validation limits, and attribution.

Channel recall additionally follows the original
[AIR FxFloorBoard source archive](https://downloads.sourceforge.net/project/fxfloorboard/AirFxFloorBoard/Desktop-FxFloorBoard/Katana_Air_source_code_Qt5_C%2B%2B.zip),
SHA-256 `3c5fc7f67e804ec3ea81b0fda58f6b56bdba50c950fca75fdb51b30c94750706`:

- `SysxIO.cpp`, `getPatchChangeMsg`: DT1 at `00 01 00 00`, payload `00 program`.
- `menuPage_midi.cpp`: the Air channel selector exposes PANEL, A1/A2/A3 and
  B1/B2/B3, with A4 and B4 omitted.
- `customControlSwitch.cpp`: those directions map to `00/01/02/03/05/06/07`.

This is static source evidence. Other files retain nine-channel legacy code and
the source is an unfinished beta; channel recall remains experimental and has
not been checked on Air hardware. Logical UI indices 0–6 are translated through
the explicit mapping; callers never send the legacy unused programs 4 or 8.

`data/katana-air.json` retains only the 480 parameters referenced by these editor
sections. Its `source` entry records the pinned commit and the SHA-256 of the
original metadata. Excluded parameters include inherited MIDI channel tables,
pedal assignments, foot volume, and unrelated system settings.

The preamp addresses at `60 00 00 51` through `60 00 00 58` follow the corrected
upstream map. Gain preserves that source's unusual calibrated labels: raw 20
is labelled 0, raw 100 is labelled 100, with additional labels beyond those
points. The source author reports checking gain against their physical amp;
we have not independently checked these labels. Other inherited options may
also have inaccurate meanings. A matching readback confirms the raw value,
not the interpretation of a parameter on every Air firmware revision.

Offline mock tests cover Air framing/checksum isolation, bounded read-only
connection/refresh, reordered and partial responses, metadata references,
permissions (including revocation during an edit), sparse channel recall,
unavailable optional globals, unknown values, and failed readback. No Air hardware was available for verification. No hardware writes
were used in agent testing.

REA also inspected three selected Gen 3 packaged JavaScript files as inert text
(`midi_connect_controller.js`, `model_info.js`, `model_controller.js`). Parent
Evidence `ev_7f83d0fa453efa91984b890e12e1c42379502f564562ce0203d8d3a8049473ea`
records artifact SHA-256
`7c85fc7c72cac72b30e4f5e34d849d2837363a667294a229796e4e44793d433f`.
This corroborates the separate Gen 3 model/channel boundary; it does not establish
Air protocol behavior. Analysis covers only those selected sources, has partial
relationship coverage, and never executed the vendor app. The full local bundle
is retained in the parent workspace at `analysis/katana-air-gen3-boundary-rea.json`.

The user-provided REA skill applies when inspecting shipped artifacts. The Air
references above are source files; they were inspected with ordinary source
analysis as prescribed by that skill. No Air runtime behavior was observed.

## Attribution

Parameter definitions and effect layouts are adapted from AIR FxFloorBoard,
Copyright 2005–2025 Colin Willcocks and Uco Mesdag, through KatanaAirDesktop.
AIR FxFloorBoard: <https://sourceforge.net/projects/air-fxfloorboard/>.
KatanaAirDesktop and this adaptation are distributed under GPL-3.0-or-later;
see `LICENSE` and `NOTICE.md`.
