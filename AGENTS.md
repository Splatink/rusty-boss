# Hardware testing constraint

The user authorizes testing against their connected Katana amplifier **only by
reading it. Never write to the amplifier during agent testing.**

- Identity requests and RQ1 parameter reads are allowed.
- No outgoing DT1, parameter changes, TEMP-knob commands, channel selection,
  program/control changes, preview, commit, reset, mute, or restore.
- Editor-mode setup/cleanup are writes too, and are forbidden. Do not use the
  GUI Connect action or `Session::connect` for agent hardware testing: the GUI's
  Protected mode permits these session writes and is NOT absolute read-only.
- Use `read-reverb --port <explicit ALSA port>` / `Session::observe` with the
  non-switchable `ReadRequestsOnly` transport for diagnostic reads. Preserve and
  inspect the packet audit; all outgoing frames must be identity/RQ1.
- Do not work around a failed read by enabling editor mode or retrying writes.
- Mock-transport unit tests and offline UI tests may exercise write code without
  an actual device. The app can retain explicitly user-operated editing features;
  the agent must not exercise them against the hardware.

See `docs/PANEL-VALUES.md` and `tests/fixtures/katana50-reverb-readonly.json` for
the authorized read-only capture, observations and coverage limits.
