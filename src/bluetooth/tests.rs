//! Runs against a private fake BlueZ service. Never opens the system bus or radio.
use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Mutex, atomic::AtomicUsize},
};
use zbus::{fdo, object_server::SignalEmitter, zvariant::OwnedObjectPath};

const ADAPTER_PATH: &str = "/org/bluez/hci0";
const DEVICE_PATH: &str = "/org/bluez/hci0/dev_01_02_03_04_05_06";
const SERVICE_PATH: &str = "/org/bluez/hci0/dev_01_02_03_04_05_06/service001";
const CHAR_PATH: &str = "/org/bluez/hci0/dev_01_02_03_04_05_06/service001/char002";
#[derive(Default)]
struct State {
    events: Mutex<Vec<String>>,
    writes: Mutex<Vec<Vec<u8>>>,
    decoder: Mutex<codec::Decoder>,
    connected: AtomicBool,
    paired: AtomicBool,
    fail_notify: AtomicBool,
    fail_write: AtomicBool,
    wrong_service: AtomicBool,
    cancel_on_write: Mutex<Option<Arc<AtomicBool>>>,
    received_messages: AtomicUsize,
}
impl State {
    fn log(&self, event: &str) {
        self.events.lock().unwrap().push(event.into());
    }
}
struct Adapter(Arc<State>);
#[zbus::interface(name = "org.bluez.Adapter1")]
impl Adapter {
    #[zbus(property)]
    fn powered(&self) -> bool {
        true
    }
    fn set_discovery_filter(&self, filter: HashMap<String, OwnedValue>) {
        self.0.log(if filter.is_empty() {
            "clear-filter"
        } else {
            "filter"
        });
    }
    fn start_discovery(&self) {
        self.0.log("scan");
    }
    fn stop_discovery(&self) {
        self.0.log("stop-scan");
    }
}
struct Peripheral(Arc<State>);
#[zbus::interface(name = "org.bluez.Device1")]
impl Peripheral {
    #[zbus(property)]
    fn connected(&self) -> bool {
        self.0.connected.load(Ordering::Relaxed)
    }
    #[zbus(property)]
    fn services_resolved(&self) -> bool {
        self.connected()
    }
    #[zbus(property)]
    fn paired(&self) -> bool {
        self.0.paired.load(Ordering::Relaxed)
    }
    #[zbus(property)]
    fn name(&self) -> &str {
        "Test amplifier MIDI"
    }
    #[zbus(property)]
    fn alias(&self) -> &str {
        "Test amplifier MIDI"
    }
    #[zbus(property)]
    fn address(&self) -> &str {
        "01:02:03:04:05:06"
    }
    #[zbus(property)]
    fn adapter(&self) -> OwnedObjectPath {
        ADAPTER_PATH.try_into().unwrap()
    }
    #[zbus(property, name = "UUIDs")]
    fn uuids(&self) -> Vec<String> {
        vec![MIDI_SERVICE.into()]
    }
    fn connect(&self) {
        self.0.log("connect");
        self.0.connected.store(true, Ordering::Relaxed);
    }
    fn disconnect(&self) {
        self.0.log("disconnect");
        self.0.connected.store(false, Ordering::Relaxed);
    }
    fn pair(&self) {
        self.0.log("pair");
        self.0.connected.store(true, Ordering::Relaxed);
        self.0.paired.store(true, Ordering::Relaxed);
    }
    fn cancel_pairing(&self) {
        self.0.log("cancel-pair");
    }
}
struct Service(Arc<State>);
#[zbus::interface(name = "org.bluez.GattService1")]
impl Service {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        if self.0.wrong_service.load(Ordering::Relaxed) {
            "0000180f-0000-1000-8000-00805f9b34fb"
        } else {
            MIDI_SERVICE
        }
    }
    #[zbus(property)]
    fn device(&self) -> OwnedObjectPath {
        DEVICE_PATH.try_into().unwrap()
    }
}
struct Characteristic(Arc<State>);
#[zbus::interface(name = "org.bluez.GattCharacteristic1")]
impl Characteristic {
    #[zbus(property, name = "UUID")]
    fn uuid(&self) -> &str {
        MIDI_CHARACTERISTIC
    }
    #[zbus(property)]
    fn service(&self) -> OwnedObjectPath {
        SERVICE_PATH.try_into().unwrap()
    }
    #[zbus(property)]
    fn flags(&self) -> Vec<&str> {
        vec!["notify", "write-without-response"]
    }
    fn start_notify(&self) -> fdo::Result<()> {
        self.0.log("subscribe");
        if self.0.fail_notify.load(Ordering::Relaxed) {
            return Err(fdo::Error::Failed("test subscribe failure".into()));
        }
        Ok(())
    }
    fn stop_notify(&self) {
        self.0.log("unsubscribe");
    }
    async fn write_value(
        &self,
        bytes: Vec<u8>,
        options: HashMap<String, OwnedValue>,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> fdo::Result<()> {
        assert_eq!(
            options.get("type").and_then(|v| <&str>::try_from(v).ok()),
            Some("command")
        );
        self.0.writes.lock().unwrap().push(bytes.clone());
        if self.0.fail_write.load(Ordering::Relaxed) {
            return Err(fdo::Error::Failed("test send failure".into()));
        }
        let frames = self
            .0
            .decoder
            .lock()
            .unwrap()
            .feed(&bytes)
            .map_err(|e| fdo::Error::Failed(e.to_string()))?;
        for frame in frames {
            self.0.received_messages.fetch_add(1, Ordering::Relaxed);
            let response = if frame == crate::air::IDENTITY_REQUEST {
                vec![0xf0, 0x7e, 0, 6, 2, 0x41, 0x50, 0, 0, 0, 0, 0, 0, 0, 0xf7]
            } else {
                frame
            };
            for packet in codec::encode(&response, 0x77, 20).unwrap() {
                let changed: HashMap<&str, Value<'_>> =
                    HashMap::from([("Value", Value::from(packet))]);
                emitter
                    .emit(
                        "org.freedesktop.DBus.Properties",
                        "PropertiesChanged",
                        &(CHARACTERISTIC, changed, Vec::<String>::new()),
                    )
                    .await
                    .map_err(|e| fdo::Error::Failed(e.to_string()))?;
            }
        }
        if let Some(cancel) = self.0.cancel_on_write.lock().unwrap().as_ref() {
            cancel.store(true, Ordering::Relaxed);
        }
        Ok(())
    }
}
struct PrivateBus {
    process: Child,
    address: String,
}
impl PrivateBus {
    fn new() -> Self {
        let mut process = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("offline D-Bus tests require dbus-daemon");
        let mut address = String::new();
        BufReader::new(process.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        assert!(address.starts_with("unix:"));
        Self {
            process,
            address: address.trim().into(),
        }
    }
}
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}
struct Rig {
    bluez: Option<Bluez>,
    _server: Connection,
    _bus: PrivateBus,
    state: Arc<State>,
    _runtime: Arc<Runtime>,
}
impl Rig {
    fn new() -> Self {
        let bus = PrivateBus::new();
        let state = Arc::new(State::default());
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .unwrap(),
        );
        let server = runtime.block_on(async {
            zbus::connection::Builder::address(bus.address.as_str())
                .unwrap()
                .name(BLUEZ)
                .unwrap()
                .serve_at(ADAPTER_PATH, Adapter(state.clone()))
                .unwrap()
                .serve_at(DEVICE_PATH, Peripheral(state.clone()))
                .unwrap()
                .serve_at(SERVICE_PATH, Service(state.clone()))
                .unwrap()
                .serve_at(CHAR_PATH, Characteristic(state.clone()))
                .unwrap()
                .serve_at("/", fdo::ObjectManager)
                .unwrap()
                .build()
                .await
                .unwrap()
        });
        let client = runtime
            .block_on(
                zbus::connection::Builder::address(bus.address.as_str())
                    .unwrap()
                    .build(),
            )
            .unwrap();
        let bluez = Bluez {
            runtime: runtime.clone(),
            connection: client,
        };
        Self {
            bluez: Some(bluez),
            _server: server,
            _bus: bus,
            state,
            _runtime: runtime,
        }
    }
    fn device(&self) -> Device {
        Device {
            id: DEVICE_PATH.into(),
            name: "Test amplifier MIDI".into(),
            address: "01:02:03:04:05:06".into(),
            adapter: ADAPTER_PATH.into(),
            paired: false,
            advertises_midi: true,
        }
    }
    fn open(&mut self, cancel: Arc<AtomicBool>) -> Result<BluetoothTransport> {
        BluetoothTransport::open_with(self.bluez.take().unwrap(), &self.device(), cancel)
    }
    fn events(&self) -> Vec<String> {
        self.state.events.lock().unwrap().clone()
    }
}

#[test]
fn native_connect_and_generic_gen3_messages_use_real_dbus_gatt_path() {
    let mut rig = Rig::new();
    let mut transport = rig.open(Arc::new(AtomicBool::new(false))).unwrap();
    assert!(
        rig.state.writes.lock().unwrap().is_empty(),
        "transport connect sends no MIDI commands"
    );
    let message = crate::protocol::Message::Data {
        device: 0x10,
        address: crate::protocol::Address::packed(0x20000600).unwrap(),
        data: vec![42; 128],
    }
    .encode()
    .unwrap();
    transport.send(&message).unwrap();
    assert_eq!(
        transport.receive(Duration::from_secs(2)).unwrap(),
        Some(message)
    );
    assert_eq!(rig.state.received_messages.load(Ordering::Relaxed), 1);
    drop(transport);
    assert_eq!(
        rig.events(),
        ["connect", "subscribe", "unsubscribe", "disconnect"]
    );
}
#[test]
fn air_session_can_use_native_transport_without_transport_knowing_air_protocol() {
    let mut rig = Rig::new();
    let transport = rig.open(Arc::new(AtomicBool::new(false))).unwrap();
    let gate = crate::safety::WriteGate::default();
    let session = crate::air::AirSession::connect(transport, gate).unwrap();
    assert!(session.is_connected());
    assert_eq!(rig.state.received_messages.load(Ordering::Relaxed), 1);
    drop(session);
    assert!(rig.events().contains(&"disconnect".into()));
}
#[test]
fn cancelled_scan_always_stops_discovery_without_connecting() {
    let rig = Rig::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let trigger = cancel.clone();
    let state = rig.state.clone();
    let thread = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline && !state.events.lock().unwrap().iter().any(|e| e == "scan")
        {
            thread::sleep(Duration::from_millis(5));
        }
        trigger.store(true, Ordering::Relaxed);
    });
    assert!(discover_with(rig.bluez.as_ref().unwrap(), &cancel, Duration::from_secs(3)).is_err());
    thread.join().unwrap();
    assert_eq!(
        rig.events(),
        ["filter", "scan", "stop-scan", "clear-filter"]
    );
    assert!(rig.state.writes.lock().unwrap().is_empty());
}
#[test]
fn discovery_returns_named_midi_devices_and_cleans_up() {
    let rig = Rig::new();
    let devices = discover_with(
        rig.bluez.as_ref().unwrap(),
        &AtomicBool::new(false),
        Duration::from_millis(10),
    )
    .unwrap();
    assert_eq!(devices, vec![rig.device()]);
    assert_eq!(
        rig.events(),
        ["filter", "scan", "stop-scan", "clear-filter"]
    );
}
#[test]
fn failed_connect_releases_subscription_and_only_connections_it_created() {
    let mut rig = Rig::new();
    rig.state.fail_notify.store(true, Ordering::Relaxed);
    assert!(rig.open(Arc::new(AtomicBool::new(false))).is_err());
    assert_eq!(
        rig.events(),
        ["connect", "subscribe", "unsubscribe", "disconnect"]
    );
    let mut rig = Rig::new();
    rig.state.connected.store(true, Ordering::Relaxed);
    let transport = rig.open(Arc::new(AtomicBool::new(false))).unwrap();
    drop(transport);
    assert_eq!(rig.events(), ["subscribe", "unsubscribe"]);
    let mut rig = Rig::new();
    rig.state.wrong_service.store(true, Ordering::Relaxed);
    assert!(rig.open(Arc::new(AtomicBool::new(false))).is_err());
    assert_eq!(rig.events(), ["connect", "disconnect"]);
}
#[test]
fn send_failures_and_cancellation_never_retry_or_finish_queued_fragments() {
    let mut rig = Rig::new();
    let mut transport = rig.open(Arc::new(AtomicBool::new(false))).unwrap();
    rig.state.fail_write.store(true, Ordering::Relaxed);
    assert!(transport.send(&[0xf0, 1, 0xf7]).is_err());
    assert!(transport.send(&[0xf0, 1, 0xf7]).is_err());
    assert_eq!(rig.state.writes.lock().unwrap().len(), 1);
    drop(transport);
    let mut rig = Rig::new();
    let cancel = Arc::new(AtomicBool::new(false));
    let mut transport = rig.open(cancel.clone()).unwrap();
    *rig.state.cancel_on_write.lock().unwrap() = Some(cancel);
    let message = [vec![0xf0], vec![1; 128], vec![0xf7]].concat();
    assert!(transport.send(&message).is_err());
    assert_eq!(rig.state.writes.lock().unwrap().len(), 1);
    drop(transport);
    assert!(rig.events().contains(&"disconnect".into()));
}
#[test]
fn idle_disconnect_is_detected_without_midi_writes() {
    let mut rig = Rig::new();
    let transport = rig.open(Arc::new(AtomicBool::new(false))).unwrap();
    rig.state.connected.store(false, Ordering::Relaxed);
    let deadline = Instant::now() + Duration::from_secs(3);
    while transport.is_connected() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(!transport.is_connected());
    assert!(rig.state.writes.lock().unwrap().is_empty());
}

#[test]
fn pairing_is_explicit_and_an_existing_bond_is_preserved() {
    let rig = Rig::new();
    let cancel = AtomicBool::new(false);
    pair_with(rig.bluez.as_ref().unwrap(), &rig.device(), &cancel).unwrap();
    pair_with(rig.bluez.as_ref().unwrap(), &rig.device(), &cancel).unwrap();
    assert_eq!(rig.events(), ["pair", "disconnect"]);
    assert!(rig.state.paired.load(Ordering::Relaxed));
    assert!(rig.state.writes.lock().unwrap().is_empty());
}
#[test]
fn existing_gen3_write_gate_operates_before_bluetooth_framing() {
    let mut rig = Rig::new();
    let transport = rig.open(Arc::new(AtomicBool::new(false))).unwrap();
    let gate = crate::safety::WriteGate::default();
    let mut transport = crate::safety::Guarded::new(transport, gate.clone());
    let packet = crate::protocol::Message::Data {
        device: 0x10,
        address: crate::protocol::Address::packed(0x20000600).unwrap(),
        data: vec![42],
    }
    .encode()
    .unwrap();
    assert!(transport.send(&packet).is_err());
    assert!(rig.state.writes.lock().unwrap().is_empty());
    gate.set(crate::safety::Access::EditBuffer);
    transport.send(&packet).unwrap();
    assert_eq!(
        transport.receive(Duration::from_secs(1)).unwrap(),
        Some(packet)
    );
}
