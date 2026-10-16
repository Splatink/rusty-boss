# Installer investigation and implementation ledger

## Boundary and current result

Requested: reverse engineer the supplied Windows/macOS installers and implement a
Linux-first Rust replacement named **Rusty Boss**, with full application behavior.

Completed here: static extraction, matching-platform UI payload comparison, focused
protocol/file-format analysis, and a buildable native Rust foundation. **Full parity
is not complete.** The per-feature checklist is [PARITY.md](PARITY.md).

No original installer, original native application, or packaged JavaScript was
executed. No recorded endpoint was fetched. Native wrapper decompilation was not
needed for the chosen protocol/file-format slice and was not performed. The new
Rust UI was launched in a bounded four-frame smoke test; that says nothing about
execution of the original app or attached-amplifier behavior.

## Exact input identities

SHA-256:

| Input | Digest |
|---|---|
| `BOSS TONE STUDIO for KATANA Gen 3 Installer.exe` | `1016f6da6de7d18e655fbddc22b90e96fb8c4804c3f716e8d17739f8c47a79ff` |
| `BOSS TONE STUDIO for KATANA Gen 3 Installer.pkg` | `ab460b6907bc769adab2236573af7f5b61f4866541d892bfc35e0a39c40e9ba9` |
| Extracted Windows application EXE | `aee8648dbcfeb4c5f2fb6825b5953c27d97d40b5164f545b7e36ec1ee6f53a71` |
| Extracted macOS universal executable | `e531158e436f70e9239c308c87d64bab73bcba5051ef008ee93ac9eb71495aed` |

All paths below are relative to the parent workspace unless otherwise stated.
Original inputs were not overwritten.

### Extraction observations

- Windows installer: 7-Zip identified an Inno Setup PE wrapper, version string
  `1.1.0 (26)`. `innoextract 1.9` extracted into `analysis/windows/` without running
  the installer. The HTML tree is under
  `localappdata/Roland/BOSS TONE STUDIO for KATANA Gen 3/html/`.
- macOS installer: XAR package, with a gzip-compressed ODC CPIO Payload. Extracted
  with 7-Zip, gzip and cpio into `analysis/pkg/` and `analysis/macos/`.
  The HTML tree is under
  `BOSS TONE STUDIO for KATANA Gen 3.app/Contents/Resources/html/`.
- 7-Zip reported 1,727 trailing bytes after the XAR physical archive. This is an
  observed container condition, not a validation of a signature or maliciousness.
- Original scripts, native bridges, graphics and fonts remain in `analysis/`, not
  in Rusty Boss's runtime. No system package manager/configuration was changed.
- A local portable extraction tool was downloaded because innoextract was absent:
  `https://constexpr.org/innoextract/files/innoextract-1.9-linux.tar.xz`.
  Archive SHA-256: `008efe5011476ccc4aae17c3e22038b5a1bc5c7aad2b9d4d869537bf3874d21f`.
  Used amd64 executable SHA-256:
  `7a8ad941deeb0de8d1f804830f339fd17e27cfbce473615d79d1d65c106884e9`.
  Kept in `analysis/tools/`; not installed globally. This records bytes, not an
  independent authentication of the tool publisher.

### Platform comparison

`evidence/platform-comparison.json` records each of the 385 macOS HTML-subtree
files, both raw file digests, and a comparison after CRLF -> LF normalization.
Every common file matched under that normalization. Five Windows-only files are
`images/chain/effectblock_amp@2x_2.png` through `_6.png`.

This is a local byte comparison, not a REA semantic version-equivalence claim.
It covers the extracted HTML trees, not native bridge, installer, OS, MIDI-driver,
network-service or audible behavior. REA's complete artifact inventories bind
both tree identities, but its selected-source analyses do not semantically cover
all files in those trees.

## Retained REA evidence

The local session was exported to `evidence/rea-bundle.json`; it is a relatively
large, git-ignored investigation artifact, not a runtime dependency. Exact IDs:

| Evidence | ID / scope |
|---|---|
| PKG inspection parent | `ev_6f929105fbd23fabe3ad380ee6ee42365324d3bc51bed9fd89b930347cbd6901` |
| PKG inventory | `ev_042b793564e63223204a7bf062966d1d5bb20e8be517448eba9f46bb9ddf2825` |
| PKG graph manifest | `agm_dc7d4b0136068675b1e20c26c896b300129eba3607d23d44e865f1dd761f0f48` |
| macOS selected JS analysis parent | `ev_66a22a6e56121331455b1edb5598ff67f8a32b1aa796fa289ffcc4802a17eb6b` |
| macOS analysis summary | `ev_fd9632ddc905c5b407d38762665e22c859f5d50d4ab6e4b5729db8c84b431a4a` |
| Windows selected JS analysis parent | `ev_944b442de6d530e1f552e1d5adbb0d232389a874846088d0b12b58122dcc255c` |
| Windows analysis summary | `ev_d603f5c9517fea59d7690abe8070f4a35e00f5c058fc19037adaffd258fa85a1` |
| Extracted Windows PE resource inspection | `ev_e43725211b27da566575adf1f8a5ec8c7b5c7d09745113054021dbc4bfe8c378` |

Mac tree REA identity:
`fcfd680fb9b2bbd9194ea1002e9f70dbacfd27881790aeb372220a0fd0c50181`.
Windows tree REA identity:
`7c85fc7c72cac72b30e4f5e34d849d2837363a667294a229796e4e44793d433f`.
These are tree identities produced by REA, not hashes of the installers.

**Coverage limits:** REA PKG inspection on Linux is root-hash-only; child
extraction observations above came from the separate format-aware local tools.
Both JavaScript analyses explicitly selected ten source paths and left 139
inventoried relevant files outside source analysis. The macOS analysis parsed ten
JS files; the Windows selection parsed nine JS files and acquired one JSON file.
There were no parse failures, but application and semantic coverage are partial.
Dynamic/global relationships and numerous semantic unknowns remain. The PE
inspection covered 22 resources/51 entries, with non-icon-group payloads opaque;
it was not a decompilation or an observation of Windows UI behavior.

## Finding ledger: observations, inferences and design choices

Paths are relative to an extracted `html/` root. Line references identify the
unmodified LF-normalized macOS copy, not Rust implementation line numbers.

### 1. Release and communications profile — observed static literals

`js/config/product_setting.js` (roughly lines 8–46) declares:

- application/device name `KATANA Gen3`; version `1.1.0`, build `26`;
- manufacturer `41` is added by common MIDI code;
- model ID `01 05 07`, default device ID `10` hex;
- four address bytes and four request-size bytes;
- 20 ms added MIDI pacing, 15 s request timeout;
- TSL and ALB revision `0000`.

Supported by both selected-JS parent Evidence records. The fact that these
literals are shipped does not establish that any particular firmware accepts them.

### 2. Roland frame and address encoding — observed source logic

`js/common/midi_controller.js` lines 25–64 and 135–161 construct:

```
DT1: F0 41 <device> 01 05 07 12 <addr[4]> <data> <checksum> F7
RQ1: F0 41 <device> 01 05 07 11 <addr[4]> <size[4]> <checksum> F7
```

Checksum is `(128 - sum(address + data/size) % 128) & 0x7f`.
`js/utilities/converter.js` defines `nibble` and `_7bitize`: addresses/sizes are
linear base-128 values in memory, not ordinary packed-hex integers. In particular
packed `20 00 00 7F` plus one becomes `20 00 01 00`.

Rust `protocol.rs` is a new implementation of those observations. It additionally
validates incoming checksums, frame lengths and byte ranges, bounds stream buffers,
reassembles split SysEx, ignores interleaved real-time bytes and rejects overflow.
These stricter checks are deliberate design choices, not a claim that the vendor
receiver implements the same validation policy.

### 3. Full shipped map, not all workflows — observed definitions

`js/config/address_map.js` parameter/group declarations and root list around
lines 1044–1178 establish:

- setup at packed `00000000`;
- system at `10000000`;
- active/edit patch at `20000000`;
- stored row roots `20100000`, `20200000` … `20700000`, `21000000`, `21100000`,
  `21200000`;
- transient values at `30000000`; command area at `7F000000`.

`js/common/parameter.js` identifies ASCII, one-byte seven-bit, two-byte
nibble/seven-bit and four-byte nibble encodings, with signed display offsets.
`export/item.json` plus `export/layout.div` provide option labels and
`list_order` mappings; `js/item/select_box_controller.js` explains how display
indices map back to raw values.

`tools/recover_schema.py` parses these declarations as inert text and derives
`data/katana-gen3.json`. It records exact source hashes and expands repeated
block templates, yielding 106 blocks and 2,463 parameter instances (not 2,463
unique algorithms/features). No JS is evaluated. The Rust tests validate every
numeric parameter at its min/default/max. The generic UI intentionally exposes
inactive-bank/algorithm data instead of claiming to recreate every vendor
visibility rule, unit conversion or coordinated control side effect.

### 4. Identity and models — observed static checks

`js/businesslogic/bts/address_const.js` and
`js/businesslogic/bts/midi_connect_controller.js` around lines 186–231 send
Universal Identity Request `F0 7E 7F 06 01 F7`, check the Roland family prefix,
read the returned device ID and a model code at reply byte offset 10, read
communication level at `7F000000`, then enable editor mode at `7F000001`.

`js/businesslogic/ktn/model_info.js` maps model codes:
05=50, 06=100, 07=100/212, 08=HEAD, 09=Artist, 0A=50 EX, 0B=Artist HEAD.
It represents PANEL plus two four-channel banks, with only two channels per bank
present on the 50. `_bid` in `converter.js` maps librarian row i to PATCH(i+1).
The tenth library row is not a selectable channel under these model masks.

Rust implements the identity/level/mode slice and channel masks, not a complete
model-capability UI. No USB identity reply was actually captured in this work.

### 5. Select, preview, commit, backup — source-supported, hardware unverified

`js/businesslogic/bts/patch_controller.js` around lines 316–389 sends:

- select: DT1 to `7F000100`, payload `00 <slot>`;
- current name: `20000000`, sixteen ASCII bytes;
- commit current patch: `7F000104`, payload `00 <slot>`;
- preview mute: `7F010109`, payload `01`;
- export/unmute: `7F01010A`, payload `40 <slot>` for the single-row unmute case.

`js/common/librarian.js`, its
`js/businesslogic/bts/librarian/librarian.js` override and
`js/businesslogic/bts/librarian/librarian_services.js` distinguish temporary
writes from bulk stored-row transfer/export. `backup/all_data.js` and
`backup/all_backup.js` include further backup/restore/export sequencing.

Rust has single-field editing, writable edit-buffer preview, channel selection
with current-number readback, current-buffer commit with address-matched command
acknowledgements and experimental backup reads. It does not claim bulk-restore
parity and does not expose that operation. Preview deliberately omits blocks
named READONLY rather than assuming that writing telemetry is safe. Side effects,
status payload meanings, failure cleanup, exact timing and sound remain unknown.
No transactional rollback is promised. Device state may change during sequential
reads; this does not produce an atomic hardware snapshot.

### 6. Interchange formats — observed JSON model; constructed-fixture tests

`js/businesslogic/bts/librarian/librarian_model.js` lines 120–196 serializes:

```
{ name, formatRev: "0000", device: "KATANA Gen3",
  data: [[ { memo, paramSet: { "PATCH%...": ["00", "32", ...] } } ]] }
```

The vendor loader fills missing/short blocks from factory defaults. Rust preserves
that tail-fill policy with warning counts, and preserves unknown document/patch
fields and unknown parameter blocks. It rejects other device names rather than
silently inventing MkI/MkII conversion.

`js/businesslogic/bts/backup/all_backup_model.js` lines 35–55 serializes ALB
`system`, `setup`, `userPatch` (including null rows), `device`, `model`, `formatRev`.
Rust implements that file shape and patch recovery. Validation also imposes a
16 MiB input limit and bounded rows/blocks as defensive design choices.
Actual vendor-exported TSL/ALB files were not present, so tests are constructed
from recovered definitions; success in the original application is not established.

### 7. Online features — endpoint observations only

`product_setting.js` contains Tone Central definition URLs and the public
`https://bosstoneexchange.com/gear/katana_gen_3` page. Rust provides a user-activated
external-browser link to the latter and local TSL import. No endpoint was fetched,
no cloud credentials were obtained, and no cloud feature parity is asserted.

### 8. Standard editor and preset protection — UI-focused continuation

The user subsequently reported that the initial app works with their amplifier.
That report is not independent observation of every hardware path. A second,
strictly offline pass recovered algorithm pages, finite display tables, banks,
chain topology and basic model capabilities, and implemented a clearer Standard
editor plus transport-level preset protection. No attached-device traffic was
sent during that pass. [UI-RECONSTRUCTION.md](UI-RECONSTRUCTION.md) records exact
new Evidence IDs, source scope, implementation choices and remaining unknowns.
The earlier generic-editor limitations above describe the initial milestone;
the raw view is now the Advanced fallback, not the default UI.

### 9. Panel-position / effect-level distinction and presentation refinement

The user reports live-buffer editing and amp preset loading work, but reverb
strength did not match the physical knob. The focused continuation found separate
read-only panel-macro positions and writable algorithm levels; it did **not**
establish a universal scale conversion. After subsequent explicit read-only
authorization, a no-DT1 diagnostic observed KATANA-50 panel position 52 with active
red SPRING Effect Level 68 and PRESET assignment. It did not measure the physical
knob angle or reproduce the exact earlier 50 reading.
[Panel values](PANEL-VALUES.md) records the new REA IDs, exact bindings, tests,
visual changes and outstanding device-validation question. The editor now shows
both concepts separately. The GUI read path is unchanged. The separate bounded
diagnostic sent only identity/RQ1 reads; no write permission was granted or used.

## Validation and continuation

Locally run: full/default and headless core Rust tests, headless egui rendering of
all patch/system blocks, clippy with warnings denied, a native desktop startup/exit
smoke test, CLI schema/new/inspect/request examples, and ALSA port enumeration
in the initial pass. The UI continuation did not enumerate or access the attached
amplifier; it added algorithm/bank invariance, routing, display and write-gate tests.
`docs/VALIDATION.md` records final commands/results.

The principal unresolved boundary remains broader real device behavior. A focused
read-only KATANA-50 reverb exchange is now retained; firmware, known-good vendor
exports and broader bounded recordings are still needed. The user's no-write
restriction applies to all further agent hardware testing.
Do not infer complete application reconstruction or safe device writes from this
static evidence or these tests. The active REA archive session is closed after the
final Evidence export; no provider database was needed or left open.
