# GPLv3 licensing and redistribution review

## 1.0.0 dependency update — 2026-10-19

**The review below is a historical snapshot, not clearance of 1.0.0.** Its old
versions, hashes, counts, missing-license-file observation and notice-collection
gaps describe the original review, not the updated checkout. The root `LICENSE`
now exists and Cargo points to it with `license-file`; the owner still needs to
confirm the application license grant/variant and contribution rights.

The current `licenses/dependencies.csv` replaces the old inventory; the historical
inventory remains available in Git. The updated graph contains **349 locked
registry packages**, **201 default Linux normal/build dependencies** and **204
including dev dependencies**. All 349 archive hashes match the lockfile; extracted
manifests and 564 notice-file candidates match their archives. Every registry
package declares a license. Declared terms remain permissive alternatives or
separately licensed fonts; `Apache-2.0 OR GPL-2.0-only` offers Apache-2.0, not a
mandatory GPLv2-only obligation. This is not an exhaustive source-level audit.

`../THIRD-PARTY-NOTICES.md` now collects default Linux dependency and font notices,
including upstream texts absent from 11 archives. Six egui-family crates record
VCS revision `7c0b40402271e8d40cda69f87cf3c2617c2e4382`, which GitHub could not
resolve during collection. Their texts are explicitly sourced from the annotated
0.37.0 release tag's commit `841a328ba2dd8f6996beb7189ebb7fc00e668a2a` instead;
exact-revision verification remains open. The bundle is collected evidence, not
proof that every embedded notice has been found. It includes the new egui-icons
font notice as well as the previous font notices. Ship it with binaries.

Current evidence SHA-256 values:

- `Cargo.lock`: `ef43f62225eab8895e7a0359c1f79edfd8a5b75d5637b480f72c1820202c9f3d`
- `licenses/dependencies.csv`: `d14899e26d830263b9bebc0b16269f99cf750e04733783c6fe4955d537719731`
- `../THIRD-PARTY-NOTICES.md`: `74da48ccd48dd974ad6d40daee5a2dcd69611247ca422b28a163e2024ca1c67f`

Vendor-data rights, owner licensing decisions and final artifact/source-availability
review are still release requirements. See [RELEASE.md](RELEASE.md).

## Historical conclusion

**Conditional yes for the application code; not yet cleared for a public release.**
No confirmed GPLv3 incompatibility was found in the reviewed Rust dependency
license declarations. The project nevertheless needs an explicit outbound license,
a complete third-party notice bundle, and a decision about the redistribution
rights in the vendor-derived data before it should be described as release-ready.
Fonts remain separately licensed assets, not GPL-relicensed code.

This is a technical licensing/provenance review, not a legal opinion or a guarantee
of non-infringement. It does not establish that AI-generated code is original or
copyrightable, clear patents or trademarks, or resolve jurisdiction-specific
reverse-engineering, contract or database-right questions. No license was applied
by this review, and no amplifier access was performed.

## Snapshot and coverage

- Application source snapshot: Git commit `5edf691` (`Updated themes`), plus the
  documentation and inventory added by this review.
- `Cargo.lock` SHA-256:
  `8ab140557b30af91711becc0b9ffc25a190ba4296def466274e0eaec948068e3`.
- **390 locked registry packages**, across platforms and including inactive
  optional dependencies, were inventoried. All are from crates.io and all declare
  a license. No Git-sourced or additional local dependency was found.
- The default `x86_64-unknown-linux-gnu` Cargo tree has **230 unique dependency
  name/version pairs on normal/build edges**, or **232 including dev edges**.
  Build dependencies and proc macros are not necessarily included in the binary.
  Target-specific Cargo trees, not the broader metadata feature union, determine
  these scope flags.
- Inspected the manifest, lockfile, Cargo packaging file list, tracked asset paths,
  all four commits reachable from the current Git refs, font notices/TTF metadata,
  schema recovery tools, embedded-file references, application screenshots and icon.
- Checked the declared license expressions against GPLv3 requirements, with closer
  inspection of font licenses, Unicode notices and conjunctive declarations.
  This is not a line-by-line provenance audit of every dependency source file.
- For all 390 packages, verified cached `.crate` archive SHA-256 values against
  the lockfile. Compared extracted manifests and **625 license/notice-file
  candidates** with the corresponding archive members; no mismatches were found.
  This verifies the evidence used here, not the originality or legal validity of
  an upstream copyright claim.

The per-package evidence is [licenses/dependencies.csv](licenses/dependencies.csv).
It records exact versions, unmodified license declarations, Linux tree membership,
registry archive checksums, notice-file candidate paths/hashes and manifest hashes.
The semicolon-separated paths and hashes correspond positionally. Empty notice
columns mean no candidate file was found, **not** permission to omit notices.
Candidate presence alone also does not prove all required notices are present.

Inventory SHA-256:
`d689d507fcf0a3311a5e598cf176d70c50670c6a3ed7ec6a99f42fe0e6186280`.

## 1. Application ownership and current license — action required

There is currently **no root project license file and no `license` or
`license-file` field in `Cargo.toml`**. `NOTICE.md` is an attribution/provenance
notice, not a grant to redistribute the application under GPL. Cargo's package
listing warns about the absent license metadata.

Only one commit author (`splatink`) appears in the four reachable commits. This is
not proof that all rights are owned by that author. No contributor agreement or
other ownership clearance was found in the tracked files. Before applying GPL:

- Confirm the right to license human contributions, reused snippets and any
  employer-owned work, including material supplied through AI prompts.
- Check the applicable AI-provider terms. AI generation does not grant rights in
  third-party material, and some output may lack copyright protection where human
  authorship is required. Do not claim that a GPL notice settles those questions.
- Choose **GPL-3.0-only** or **GPL-3.0-or-later**, add the unmodified license text
  and appropriate application notices, and document the third-party asset
  exceptions. Ensure Cargo metadata accurately describes the packaged code and
  bundled Russo One font; do not describe that font as GPL or offer it under an
  alternative GPL license.

GPLv3, rather than GPLv2-only, is the appropriate compatibility target for the
current dependencies: several use Apache-2.0 without an MIT alternative.

## 2. Rust dependency compatibility — no confirmed GPLv3 conflict

The locked declarations are predominantly MIT/Apache-2.0 alternatives or MIT.
The other declared terms are Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib,
0BSD, BSL-1.0 (Boost), CC0-1.0, Unlicense, Unicode-3.0 and the font licenses below.
There is no AGPL-only, GPLv2-only, non-commercial, source-available-only or
unidentified custom license in the **declared Cargo license fields**. This finding
does not rule out notices embedded in individual source/data files.

Important details:

| Case | Assessment / obligation |
|---|---|
| MIT, BSD, ISC, Zlib, 0BSD, Boost | No GPLv3 conflict identified; preserve the applicable copyright, permission, disclaimer and other required notices. GPL does not erase these obligations. |
| Apache-2.0-only packages | GPLv3-compatible; preserve license/attribution and applicable `NOTICE` material and mark modifications as required. Examples on the Linux tree include `winit`, `glutin`, `gl_generator`, `ab_glyph` and their related crates. |
| `dpi 0.1.2`: `Apache-2.0 AND MIT` | A conjunction, not a choice. Preserve both sets of applicable terms/notices. Both are compatible with GPLv3. |
| `unicode-ident 1.0.27`: `(MIT OR Apache-2.0) AND Unicode-3.0` | Choosing MIT or Apache does not remove the Unicode notice obligation. |
| 18 Unicode-3.0-only packages | Mostly ICU4X-related data/code. The inspected license is permissive and requires its copyright/permission notice in copies or associated documentation. Retain package-specific notices, including ICU ancestry notices where supplied. |
| `r-efi 5.3.0` / `6.0.0` | All-platform lockfile entries offer `MIT OR Apache-2.0 OR LGPL-2.1-or-later`. The LGPL branch is not mandatory; a permissive alternative is available. They are not on this default Linux tree. |
| Legacy declarations such as `MIT/Apache-2.0` or `Unlicense/MIT` | Recorded verbatim rather than silently parsed as modern SPDX. Check the supplied dual-license terms when generating notices; retaining both texts where necessary avoids treating a slash as an automatic waiver. |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | Alternative grants are available; no GPLv3 conflict identified. Do not drop notices for whichever grant is used. |
| CC0 / Unlicense | No copyright-license incompatibility identified. CC0 does not itself provide a patent grant; this is not a patent clearance. |

### Missing release notice bundle

The repository does **not** currently ship a comprehensive Rust dependency notice
bundle. `NOTICE.md` points to crate metadata, but metadata/SPDX identifiers are
not a replacement for required copyright and license texts in a distributed app.
The CSV is an audit inventory, **not** a complete redistribution notice bundle.

The filename-based inspection found **56 of the 390 archives** with no standalone
license/notice-file candidate. **10 are on the default Linux normal/build tree**:

- `ecolor`, `eframe`, `egui`, `egui-winit`, `egui_glow`, `emath`, `epaint` — all
  `0.31.1`;
- `gl_generator 0.14.0`;
- `khronos_api 3.1.0`;
- `profiling 1.0.18`.

Their license declarations are present; this is an evidence/notice-collection gap,
not a finding that they are unlicensed. Recover applicable license and attribution
texts from the **matching upstream revisions** (use each crate's VCS metadata),
including notices embedded in source or vendored registry files. Do not blindly
substitute a current upstream license or assume that the 625 candidates cover all
vendored materials. Generate and inspect a notice bundle for each released build.

## 3. Fonts — permitted bundling, separate licenses and missing notices

`Cargo.toml` enables eframe's `default_fonts`. Inspection of
`epaint_default_fonts 0.31.1/src/lib.rs` and the font name tables confirms that the
application's font inventory is larger than just its header font:

| Font | Copyright / terms | Current status |
|---|---|---|
| Russo One | Jovanny Lemonad, 2011–2012; SIL OFL 1.1; Reserved Font Name `Russo` | Unmodified TTF is in `assets/fonts/`. Copyright/license file is shipped in the source and exposed in Settings. Hash matches the documented pinned font. Keep this treatment in releases. |
| Ubuntu Light | Canonical Ltd., 2011; Ubuntu Font Licence 1.0 | Embedded through egui; corresponding readable notice is not currently shipped in this repository/app's license display. |
| Noto Emoji | Google Inc., 2013 (from this TTF's copyright field); SIL OFL 1.1 | Embedded through egui; its copyright and OFL notice are not currently included in the app's notice display/release bundle. |
| Hack Regular | Source Foundry Authors, 2018, MIT; Bitstream, 2003, Bitstream Vera license; DejaVu contributions identified as public domain | Embedded through egui. The crate's font notice contains **MIT plus Bitstream Vera terms**, a detail not expressed fully by the crate's declared license expression. Include the entire relevant notice, not merely MIT. |
| `emoji-icon-font` | John Slegers, 2014; MIT | Embedded through egui; preserve `fonts/emoji-icon-font-mit-license.txt`. |

The crate's declared expression is
`(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`.
Its notice sources include `fonts/Hack-Regular.txt`, `fonts/OFL.txt`,
`fonts/UFL.txt`, the icon-font MIT file, and the font copyright metadata.

These font licenses explicitly allow bundling/embedding with software, subject to
their terms. This supports a GPLv3 application **with separately licensed font
assets**, not a claim that all embedded bytes are GPL-relicensable. Preserve font
copyright/license notices and naming restrictions; do not modify or relicense the
fonts under GPL. Ubuntu's license also imposes naming rules on modified versions.
No font replacement or application behavior change was made during this review.

Russo One TTF SHA-256:
`bc0abcc660bd8b7ad3000ecb2898a27c58a29a50f7ec81652fa12e75148d09df`.
An independent download from the pinned Google Fonts commit
`14f3bd097a44db854285fae061ae0fe6eb1a583b` matched the TTF byte-for-byte.
Its upstream OFL notice also matches locally except for one removed trailing
space; the copyright, reserved name and license terms are unchanged.

## 4. BOSS/Roland-derived material — unresolved rights question

### Good separation already present

- No original installer, extracted application JavaScript, vendor native bridge,
  vendor font or standalone vendor artwork was found among the current tracked
  file paths or the Cargo source-package listing. The screenshot assets inspected
  show Rusty Boss's own interface; the desktop SVG is a simple original-style
  knob/`RB` graphic, not an imported BOSS logo.
- A history path check of all four reachable commits found none of those original
  binary/script artifacts. This was not a forensic similarity scan of every blob.
- Local `evidence/*.json` files are ignored and absent from both the tracked list
  and Cargo's package list. They contain investigation material and must stay out
  of public release archives. Vendor installers and extracted resources in the
  parent workspace must also stay out. A tarball of the whole checkout/workspace
  would bypass these boundaries.
- No tone-library `.tsl`/`.alb` collection is tracked. The read-only hardware test
  fixture is a diagnostic state/packet capture, not a redistributed preset library.

### What cannot be cleared by a dependency scan

Both `data/katana-gen3.json` and `data/editor-ui.json` are embedded in the binary.
Inspection of the recovery tools and source digests establishes direct extraction
from the supplied vendor application. The outputs include not only numeric
addresses, sizes and ranges, but also parameter identifiers, option labels,
widget/group/page identifiers and **routing/chain tables**. In particular,
`recover_editor.py` parses the vendor's chain lists into the shipped JSON.

Protocol facts may be uncopyrightable or usable for interoperability in relevant
jurisdictions. That does **not** establish blanket permission for copied text,
selection/arrangement, data collections or contractual restrictions. The existing
`NOTICE.md` factual-interoperability description is useful provenance, not a legal
clearance. This was not a formal independent clean-room implementation.

Before broad redistribution, obtain appropriate rights/legal review for these
outputs, or replace questionable copied expression/grouping with independently
authored, validated interoperability descriptions. Whether either step is legally
necessary depends on the material and jurisdiction; this review cannot decide it.
No installer EULA, anti-circumvention analysis, database-right analysis or patent
clearance was completed here. GPL cannot cure missing third-party rights.

Roland/BOSS names, the project name and theme references should also be assessed
separately for trademark/endorsement issues. An independent-project disclaimer is
present, but is not trademark permission.

## 5. Native libraries, toolchain and distribution

The current release executable directly links to system `libasound.so.2`,
`libgcc_s.so.1`, `libm.so.6`, `libc.so.6` and the ELF loader (checked with `ldd`).
The installed ALSA library is `1.2.16.1`, identified by the system package as
LGPL-2.1-or-later. The Rust `alsa` crate's permissive grant does not replace the
native library's license. No native shared-library copies are shipped in this
repository's source package.

Dynamic use of stock system libraries is not a confirmed GPLv3 blocker. If a
release bundles native libraries, a container/AppImage/runtime, or static copies,
review their own notices, source/relinking obligations and any relevant exceptions
for that actual artifact. Do not interpret a distribution package's aggregate
license expression as the exact license of every `.so` within it.

`ldd` is not exhaustive: windowing/graphics libraries may be loaded dynamically,
and portal services run separately. The Rust standard library and compiler/runtime
support also sit outside `Cargo.lock`. Rust 1.99.0's installed
`share/doc/rust/COPYRIGHT-library.html` and `licenses/` provide starting evidence;
they were located but not exhaustively audited or bundled by this pass. A final
binary/container notice and provenance review must cover the actual toolchain and
any additional copied native assets. This review did not certify a release artifact.

## Release checklist

1. **Owner decision:** confirm contribution/AI-input rights and choose the outbound
   GPLv3 variant. Add the license grant/text and accurate metadata, with explicit
   third-party asset boundaries.
2. **Vendor-derived data:** resolve the rights/provenance question for both embedded
   JSON files; do not rely only on the word "facts" in a notice.
3. **Notices:** collect exact-revision Rust/dependency and embedded-font notices,
   including the missing upstream texts and Hack/Bitstream terms. Ship them with
   binaries in a readable form; Russo One alone is insufficient.
4. **Source availability:** for a GPL binary release, provide the corresponding
   application source and necessary build scripts/materials for the released
   version, with its notices and modifications. Review GPLv3 section 6 obligations
   for the chosen delivery method and Installation Information where applicable.
5. **Artifact check:** inspect the final source/binary archive, not just this
   checkout. Exclude private evidence, installers, extracted resources and personal
   presets. Prefer a reviewed VCS/Cargo file list over a whole-workspace tarball.
6. **Re-run on change:** repeat dependency, asset and native-library review when
   versions, features, platform targets, fonts or packaging change.

## Verification commands and reference sources

The review used `cargo fetch --locked` to obtain the otherwise uncached
cross-platform archives (no builds or amplifier connection), followed by:

```sh
cargo metadata --locked --offline --format-version 1
cargo tree --locked --offline --target x86_64-unknown-linux-gnu \
  --edges normal,build --prefix none --format '{p}'
cargo tree --locked --offline --target x86_64-unknown-linux-gnu \
  --edges normal,build,dev --prefix none --format '{p}'
cargo package --list --allow-dirty --locked --offline
git ls-files
git log --format='%an <%ae>'
git rev-list --all
ldd target/release/rusty-boss
sha256sum Cargo.lock assets/fonts/RussoOne-Regular.ttf docs/licenses/dependencies.csv
```

Primary/reference material consulted:

- [GNU GPLv3 text](https://www.gnu.org/licenses/gpl-3.0.html), especially sections
  1, 4–7 and 10 concerning source, conveying and retained legal notices.
- [FSF license list: Apache-2.0](https://www.gnu.org/licenses/license-list.html#apache2)
  and its BSD/Boost/Unicode/CC0/font entries. Apache-2.0 is compatible with GPLv3,
  not GPLv2-only.
- [SIL OFL FAQ](https://openfontlicense.org/ofl-faq/), particularly the software
  bundling and mobile-application notice guidance; the actual bundled license
  text is authoritative for each font.
- [Canonical Ubuntu Font Licence](https://ubuntu.com/legal/font-licence), and the
  exact `UFL.txt` accompanying `epaint_default_fonts 0.31.1`.
- Exact crates.io manifests, crate archives and notice files identified in the CSV,
  rather than assuming today's upstream branch matches the lockfile.
