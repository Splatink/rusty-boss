# Native Bluetooth MIDI (experimental)

The `katana-air` branch connects directly to Bluetooth Low Energy MIDI through
Linux BlueZ's standard D-Bus GATT interfaces. It does not require BlueZ's optional
ALSA MIDI plugin, a MIDI bridge, a special BlueZ build, or manual port addresses.
Bluetooth support is enabled in the default build. USB remains usable when no
Bluetooth adapter or service is available.

## Using an Air

1. Turn on Bluetooth on the computer and power on the Air. Close other amp
   editors that may already be connected.
2. Select **KATANA-AIR · Experimental** in Rusty Boss.
3. Open **Settings → Find Bluetooth amplifiers**. Choose **KATANA-AIR MIDI**
   from the results and click **Connect Air**.
4. If the amp requires a bond, use **Pair device…**, respond to the normal
   desktop Bluetooth prompt, and connect again. Existing bonds are preserved.
5. Connection reads the Air identity and current sound. Enable **Preview & edit**
   to edit the sound or recall a channel. The existing protection controls apply.

Discovery lasts five seconds and can be cancelled. It lists devices advertising
the standard MIDI service, plus devices whose advertised name ends in ` MIDI`
(some publish the service only after connecting). Discovery does not connect or
pair devices. The amp session verifies device identity after connection, so a
MIDI device appearing in this list does not mean its amplifier protocol is supported.

Nothing scans, connects, pairs or reconnects automatically. If the connection is
lost, the UI clears the snapshot and revokes write permission; connect again to
read the current sound. Bluetooth setup includes the GATT notification subscription
needed to receive MIDI. Air's subsequent protocol handshake sends only identity
and parameter reads, with no amp editor-mode commands.

The collapsed **Advanced: existing ALSA MIDI port** section retains the earlier
sequencer connection for users who already have an external MIDI transport.
USB audio and **KATANA-AIR Audio** are separate from this MIDI connection.

## Requirements and troubleshooting

A working Bluetooth LE adapter, the standard BlueZ daemon and the normal desktop
Bluetooth permissions are required. Enable Bluetooth in system settings if the
app reports no powered adapter. A pairing operation uses the desktop's default
Bluetooth agent; respond to its prompt. The app does not install services, change
system configuration, mark devices trusted, or remove existing pairings.

If the MIDI device is absent, keep the amp nearby, close phone/desktop editors,
and search again. A failed write is not retried automatically. Refresh or reconnect
before further edits if verification fails. Air support remains hardware-unverified;
simulated tests establish packet and lifecycle behavior, not real-device interoperability.

## Reusable transport boundary

`src/bluetooth/` contains **no amplifier model IDs, parameter addresses or commands**:

- `discover` returns device descriptors across powered adapters.
- `pair` requests an explicit bond through the desktop agent.
- `BluetoothTransport` implements the same `session::Transport` interface used
  by USB/ALSA, including cancellation and asynchronous disconnect detection.
- `codec` handles BLE MIDI 1.0 timestamps, short MIDI messages, running status,
  real-time interleaving and fragmented SysEx. It keeps the timestamp and EOX
  together, uses conservative 20-byte GATT values, and bounds incoming buffers.

Air uses `AirSession<Box<dyn Transport>>`; its protocol and parameter map are
unchanged by the connection choice. Gen 3 `Session` and `WriteGate` can use the
same transport later. Offline tests already pass Gen 3 SysEx and the existing
Gen 3 write gate through the native GATT path. The Gen 3 GUI continues to use USB
on this branch: enabling Bluetooth for those models still requires its connection
picker and model/adapter hardware validation. Other amplifier families also need
their own protocol and capability profiles. Reusing this transport does not imply
support for every Bluetooth amplifier, proprietary service or MIDI 2.0 endpoint.

BlueZ owns the radio and link encryption. Rusty Boss owns its discovery sessions,
notification subscription and connections it creates. Cleanup stops notifications
and disconnects app-created links; pre-existing connections are left connected.
Operations have bounded timeouts. Cancellation interrupts waits and stops subsequent
packet fragments. Receive overflow invalidates the connection rather than dropping
data silently. Incoming MIDI is delivered in notification order; timestamps are
removed for the editor's request/response interface, not used for musical scheduling.

## Verification and sources

Offline tests start a **private D-Bus daemon and simulated BlueZ service**, never
the system Bluetooth daemon. They exercise actual D-Bus discovery, service/characteristic
selection, subscriptions, packet writes/notifications, cancellation, cleanup,
read-only Air identity exchange, generic Gen 3 SysEx, write gates and idle disconnects.
`dbus-daemon` is required for these tests. A restrictive execution sandbox must
allow the test daemon to create its private Unix socket. No radio, amplifier,
external network or session/system bus is used by these tests.

Protocol sources:

- [BlueZ Adapter API](https://bluez.readthedocs.io/en/latest/adapter-api/): LE
  discovery, filters and ownership of discovery sessions.
- [BlueZ Device API](https://bluez.readthedocs.io/en/latest/device-api/): connection,
  service resolution and pairing through the desktop agent.
- [BlueZ GATT API](https://bluez.readthedocs.io/en/latest/gatt-api/): characteristic
  flags, `WriteValue`, `StartNotify`, `StopNotify`, and value notifications.
- [Silicon Labs BLE MIDI description](https://docs.silabs.com/bluetooth/2.13/bluetooth-code-examples-applications/midi-over-ble):
  standard service/characteristic UUIDs and timestamp fields.
- [Microsoft BLE MIDI transport architecture](https://microsoft.github.io/MIDI/kb/ble-midi-transport-architecture/):
  SysEx continuation, running-status packet boundaries and real-time handling.
