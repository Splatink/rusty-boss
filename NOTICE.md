# Notices

Rusty Boss is an independent reconstruction. Roland and BOSS names identify the
devices and applications being studied; they are trademarks of their respective
owners. No endorsement or affiliation is implied.

The Rust/Python application implementation in this project is newly written.
`data/katana-gen3.json` records protocol addresses, numeric limits/defaults,
parameter identifiers and option labels derived from the user-supplied application
packages. Its source-file digests are included. These factual interoperability
records are distinct from the original application's executable code and artwork.

The original installers and extracted JavaScript/resources are retained separately
as analysis inputs in the parent workspace. They are not required to execute the
Rust application and should not be redistributed as part of it without appropriate
rights. No original application license, artwork, fonts, branding assets or native
bridge has been relicensed by this project.

The header uses the unmodified **Russo One** typeface by Jovanny Lemonad,
copyright (c) 2011–2012, under the SIL Open Font License 1.1 (Reserved Font Name:
"Russo"). It is unrelated to the original BOSS application. The complete notice
and license are in `assets/fonts/OFL-RussoOne.txt` and embedded in the app's
Settings → Help & connection log → Typeface license. See `assets/fonts/README.md`
for the pinned source and file hash.

Third-party Rust dependencies retain their own licenses. The default Linux
normal/build dependency and embedded-font notices are collected in
`THIRD-PARTY-NOTICES.md`; ship that document and `LICENSE` with binary releases.
The all-platform locked dependency inventory is in `docs/licenses/dependencies.csv`.
The notice bundle identifies exact upstream sources and explicitly labels six
egui release-tag fallbacks where published VCS revisions were unavailable.
These collected notices do not resolve the vendor-derived data rights question
or replace a release-specific legal/provenance review. See `docs/RELEASE.md`.

## KATANA-AIR parameter definitions and layouts

`data/katana-air.json` contains parameter definitions and effect layouts adapted
from [KatanaAirDesktop](https://github.com/npsfsz/KatanaAirDesktop), commit
`a442bffe4e09941be0c75265993932ee9ac19f63`, distributed under GPL-3.0-or-later.
Its parameter map and effect layouts were adapted from AIR FxFloorBoard,
Copyright 2005–2025 Colin Willcocks and Uco Mesdag.
Upstream project: https://sourceforge.net/projects/air-fxfloorboard/.
The upstream layout files permit GNU GPL version 2 or later; its distributed
license text and KatanaAirDesktop use GNU GPL version 3. This adaptation is
provided under GNU GPL version 3 or later; the complete GPLv3 is in `LICENSE`.
Source provenance and experimental limitations are in `docs/KATANA-AIR.md`.
