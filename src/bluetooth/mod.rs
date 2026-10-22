//! Reusable Linux BLE MIDI transport using standard BlueZ GATT, independent of
//! amplifier protocols and the optional BlueZ ALSA MIDI plugin. No startup scan.
mod codec;

use crate::session::Transport;
use anyhow::{Context, Result, bail, ensure};
use futures_util::{
    StreamExt,
    future::{Either, select},
    pin_mut,
};
use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    thread,
    time::{Duration, Instant},
};
use tokio::{
    runtime::{Builder, Runtime},
    task::JoinHandle,
};
use zbus::{
    Connection, MatchRule, MessageStream, Proxy,
    fdo::{ManagedObjects, ObjectManagerProxy},
    message::Type,
    zvariant::{OwnedValue, Value},
};

pub const MIDI_SERVICE: &str = "03b80e5a-ede8-4b33-a751-6ce34ec4c700";
pub const MIDI_CHARACTERISTIC: &str = "7772e5db-3868-4112-a1a9-f2669d106bf3";
const BLUEZ: &str = "org.bluez";
const DEVICE: &str = "org.bluez.Device1";
const ADAPTER: &str = "org.bluez.Adapter1";
const SERVICE: &str = "org.bluez.GattService1";
const CHARACTERISTIC: &str = "org.bluez.GattCharacteristic1";
const CALL_TIMEOUT: Duration = Duration::from_secs(5);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);
// ATT's minimum MTU is 23. Conservative 20-byte values work without an MTU
// extension and avoid long GATT writes (which are not MIDI event packets).
const PACKET_CAPACITY: usize = 20;

type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub address: String,
    pub adapter: String,
    pub paired: bool,
    pub advertises_midi: bool,
}

async fn bounded<T>(
    cancel: &AtomicBool,
    timeout: Duration,
    operation: impl Future<Output = Result<T>>,
) -> Result<T> {
    let operation = tokio::time::timeout(timeout, operation);
    pin_mut!(operation);
    loop {
        ensure!(
            !cancel.load(Ordering::Relaxed),
            "Bluetooth operation cancelled"
        );
        let tick = tokio::time::sleep(Duration::from_millis(20));
        pin_mut!(tick);
        match select(operation.as_mut(), tick).await {
            Either::Left((result, _)) => return result.context("Bluetooth operation timed out")?,
            Either::Right(_) => {}
        }
    }
}

struct Bluez {
    runtime: Arc<Runtime>,
    connection: Connection,
}
impl Bluez {
    fn system(cancel: &AtomicBool) -> Result<Self> {
        let runtime = Arc::new(
            Builder::new_multi_thread()
                .worker_threads(1)
                .enable_all()
                .build()?,
        );
        let connection = runtime.block_on(bounded(cancel, CALL_TIMEOUT, async {
            zbus::connection::Builder::system()?.method_timeout(Duration::from_secs(60)).build().await.map_err(Into::into)
        })).context("Bluetooth service unavailable. Turn on Bluetooth and check that BlueZ is installed and running")?;
        Ok(Self {
            runtime,
            connection,
        })
    }
    fn run<T>(
        &self,
        cancel: &AtomicBool,
        timeout: Duration,
        operation: impl Future<Output = Result<T>>,
    ) -> Result<T> {
        self.runtime.block_on(bounded(cancel, timeout, operation))
    }
    async fn proxy(&self, path: &str, interface: &'static str) -> Result<Proxy<'static>> {
        Ok(zbus::proxy::Builder::new(&self.connection)
            .destination(BLUEZ)?
            .path(path.to_owned())?
            .interface(interface)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?)
    }
    async fn objects(&self) -> Result<ManagedObjects> {
        Ok(ObjectManagerProxy::builder(&self.connection)
            .destination(BLUEZ)?
            .path("/")?
            .build()
            .await?
            .get_managed_objects()
            .await?)
    }
    async fn cleanup_call(&self, path: &str, interface: &'static str, method: &'static str) {
        let _ = tokio::time::timeout(Duration::from_secs(1), async {
            self.proxy(path, interface)
                .await?
                .call::<_, _, ()>(method, &())
                .await?;
            anyhow::Ok(())
        })
        .await;
    }
}
fn text_property(properties: &Properties, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(str::to_owned)
}
fn path_property(properties: &Properties, name: &str) -> Option<String> {
    properties
        .get(name)
        .and_then(|v| <&zbus::zvariant::ObjectPath<'_>>::try_from(v).ok())
        .map(ToString::to_string)
}
fn bool_property(properties: &Properties, name: &str) -> bool {
    properties
        .get(name)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
fn strings_property(properties: &Properties, name: &str) -> Vec<String> {
    properties
        .get(name)
        .and_then(|v| v.try_clone().ok())
        .and_then(|v| Vec::<String>::try_from(v).ok())
        .unwrap_or_default()
}
fn devices(objects: &ManagedObjects) -> Vec<Device> {
    let mut found = Vec::new();
    for (path, interfaces) in objects {
        let Some(properties) = interfaces.get(DEVICE) else {
            continue;
        };
        let name = text_property(properties, "Name")
            .or_else(|| text_property(properties, "Alias"))
            .unwrap_or_else(|| "Bluetooth MIDI device".into());
        let advertises_midi = strings_property(properties, "UUIDs")
            .iter()
            .any(|u| u.eq_ignore_ascii_case(MIDI_SERVICE));
        // Some devices advertise their MIDI name before publishing the service.
        // Device-specific compatibility is checked by the amplifier session.
        if !advertises_midi && !name.to_uppercase().ends_with(" MIDI") {
            continue;
        }
        let Some(address) = text_property(properties, "Address") else {
            continue;
        };
        found.push(Device {
            id: path.to_string(),
            name,
            address,
            adapter: path_property(properties, "Adapter").unwrap_or_default(),
            paired: bool_property(properties, "Paired"),
            advertises_midi,
        });
    }
    found.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    found
}

/// User-initiated bounded BLE discovery. Does not connect to or pair any device.
/// Only this D-Bus client's discovery sessions are stopped on cancellation/error.
pub fn discover(cancel: Arc<AtomicBool>) -> Result<Vec<Device>> {
    let bluez = Bluez::system(&cancel)?;
    discover_with(&bluez, &cancel, Duration::from_secs(5))
}
fn discover_with(bluez: &Bluez, cancel: &AtomicBool, duration: Duration) -> Result<Vec<Device>> {
    let mut started = Vec::new();
    let result = bluez.run(cancel, duration + Duration::from_secs(15), async {
        let objects = bluez.objects().await?;
        let adapters: Vec<_> = objects.iter().filter_map(|(path, interfaces)| {
            interfaces.get(ADAPTER).filter(|p| bool_property(p, "Powered")).map(|_| path.to_string())
        }).collect();
        ensure!(!adapters.is_empty(), "No powered Bluetooth adapter found. Turn on Bluetooth in system settings, then scan again");
        for path in adapters {
            let proxy = bluez.proxy(&path, ADAPTER).await?;
            let filter: HashMap<&str, Value<'_>> = HashMap::from([("Transport", Value::from("le"))]);
            proxy.call::<_, _, ()>("SetDiscoveryFilter", &(filter,)).await?;
            // Track before awaiting: a cancelled StartDiscovery may have already
            // succeeded remotely even though the reply did not arrive.
            started.push(path);
            proxy.call::<_, _, ()>("StartDiscovery", &()).await.context("Bluetooth scan could not start")?;
        }
        tokio::time::sleep(duration).await;
        Ok(devices(&bluez.objects().await?))
    });
    bluez.runtime.block_on(async {
        for path in &started {
            bluez.cleanup_call(path, ADAPTER, "StopDiscovery").await;
            let _ = tokio::time::timeout(Duration::from_secs(1), async {
                let filter = HashMap::<&str, Value<'_>>::new();
                bluez
                    .proxy(path, ADAPTER)
                    .await?
                    .call::<_, _, ()>("SetDiscoveryFilter", &(filter,))
                    .await?;
                anyhow::Ok(())
            })
            .await;
        }
    });
    result
}

/// Optional in-app pairing, handled by the normal desktop Bluetooth agent. No
/// trust changes, device removal or unsolicited pairing prompts are performed.
pub fn pair(device: &Device, cancel: Arc<AtomicBool>) -> Result<()> {
    let bluez = Bluez::system(&cancel)?;
    pair_with(&bluez, device, &cancel)
}
fn pair_with(bluez: &Bluez, device: &Device, cancel: &AtomicBool) -> Result<()> {
    let mut pairing_link = false;
    let result = bluez.run(cancel, Duration::from_secs(45), async {
        let proxy = bluez.proxy(&device.id, DEVICE).await?;
        if !proxy.get_property::<bool>("Paired").await? {
            pairing_link = !proxy.get_property::<bool>("Connected").await?;
            proxy.call::<_, _, ()>("Pair", &()).await.context("Pairing failed. Respond to the desktop Bluetooth prompt, or pair in system settings")?;
        }
        Ok(())
    });
    bluez.runtime.block_on(async {
        if result.is_err() {
            bluez
                .cleanup_call(&device.id, DEVICE, "CancelPairing")
                .await;
        }
        // Pair can establish a link itself. Leave the bond, but release a new
        // setup link so a later Connect owns its connection and cleanup.
        if pairing_link {
            bluez.cleanup_call(&device.id, DEVICE, "Disconnect").await;
        }
    });
    result
}

struct Endpoint {
    path: String,
    write_type: &'static str,
}
fn endpoint(objects: &ManagedObjects, device: &str) -> Result<Endpoint> {
    let services: Vec<_> = objects
        .iter()
        .filter_map(|(path, interfaces)| {
            let properties = interfaces.get(SERVICE)?;
            (path_property(properties, "Device").as_deref() == Some(device)
                && text_property(properties, "UUID")
                    .is_some_and(|u| u.eq_ignore_ascii_case(MIDI_SERVICE)))
            .then_some(path.to_string())
        })
        .collect();
    for (path, interfaces) in objects {
        let Some(properties) = interfaces.get(CHARACTERISTIC) else {
            continue;
        };
        if !text_property(properties, "UUID")
            .is_some_and(|u| u.eq_ignore_ascii_case(MIDI_CHARACTERISTIC))
            || !path_property(properties, "Service").is_some_and(|p| services.contains(&p))
        {
            continue;
        }
        let flags = strings_property(properties, "Flags");
        ensure!(
            flags.iter().any(|f| f == "notify" || f == "indicate"),
            "Bluetooth MIDI characteristic does not support notifications"
        );
        let write_type = if flags.iter().any(|f| f == "write-without-response") {
            "command"
        } else if flags.iter().any(|f| f == "write") {
            "request"
        } else {
            bail!("Bluetooth MIDI characteristic is not writable");
        };
        return Ok(Endpoint {
            path: path.to_string(),
            write_type,
        });
    }
    bail!(
        "Selected device does not expose a compatible Bluetooth MIDI service. Choose its MIDI connection, close other editors and try again"
    )
}

/// A byte-stream MIDI transport. It implements the same interface as USB/ALSA;
/// any compatible amplifier protocol can use it without changing BLE code.
pub struct BluetoothTransport {
    bluez: Bluez,
    device: String,
    endpoint: Option<Endpoint>,
    connected_by_us: bool,
    notifying: bool,
    cancel: Arc<AtomicBool>,
    lost: Arc<AtomicBool>,
    notifications: Receiver<Result<Vec<u8>, String>>,
    watcher: Option<JoinHandle<()>>,
    monitor: Option<JoinHandle<()>>,
    decoder: codec::Decoder,
    frames: std::collections::VecDeque<Vec<u8>>,
    started: Instant,
    next_send: Instant,
}
impl BluetoothTransport {
    pub fn open(device: &Device, cancel: Arc<AtomicBool>) -> Result<Self> {
        Self::open_with(Bluez::system(&cancel)?, device, cancel)
    }
    fn open_with(bluez: Bluez, device: &Device, cancel: Arc<AtomicBool>) -> Result<Self> {
        let (tx, notifications) = mpsc::sync_channel(256);
        let mut transport = Self {
            bluez,
            device: device.id.clone(),
            endpoint: None,
            connected_by_us: false,
            notifying: false,
            cancel,
            lost: Arc::new(AtomicBool::new(false)),
            notifications,
            watcher: None,
            monitor: None,
            decoder: codec::Decoder::default(),
            frames: Default::default(),
            started: Instant::now(),
            next_send: Instant::now(),
        };
        transport.bluez.run(&transport.cancel, CONNECT_TIMEOUT, async {
            let proxy = transport.bluez.proxy(&transport.device, DEVICE).await?;
            if !proxy.get_property::<bool>("Connected").await? {
                transport.connected_by_us = true;
                proxy.call::<_, _, ()>("Connect", &()).await.context("Could not connect Bluetooth MIDI. Keep the amp nearby and close other editors")?;
            }
            while !proxy.get_property::<bool>("ServicesResolved").await? {
                ensure!(proxy.get_property::<bool>("Connected").await?, "Bluetooth device disconnected during service discovery");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            transport.endpoint = Some(endpoint(&transport.bluez.objects().await?, &transport.device)?);
            Ok(())
        })?;
        let characteristic = transport.endpoint.as_ref().unwrap().path.clone();
        // Install the notification match before StartNotify so that an immediate
        // notification cannot fall between subscription and receiver setup.
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender(BLUEZ)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?
            .path(characteristic.clone())?
            .build();
        let mut messages = transport
            .bluez
            .run(&transport.cancel, CALL_TIMEOUT, async {
                Ok(
                    MessageStream::for_match_rule(rule, &transport.bluez.connection, Some(512))
                        .await?,
                )
            })?;
        let lost = transport.lost.clone();
        transport.watcher = Some(transport.bluez.runtime.spawn(async move {
            while let Some(message) = messages.next().await {
                let packet = (|| -> Result<Option<Vec<u8>>> {
                    let message = message?;
                    let (interface, mut changed, _): (String, Properties, Vec<String>) =
                        message.body().deserialize()?;
                    if interface != CHARACTERISTIC {
                        return Ok(None);
                    }
                    Ok(changed
                        .remove("Value")
                        .map(Vec::<u8>::try_from)
                        .transpose()?)
                })();
                match packet {
                    Ok(Some(packet)) => {
                        if tx.try_send(Ok(packet)).is_err() {
                            lost.store(true, Ordering::Relaxed);
                            return;
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        lost.store(true, Ordering::Relaxed);
                        let _ = tx.try_send(Err(error.to_string()));
                        return;
                    }
                }
            }
            lost.store(true, Ordering::Relaxed);
        }));
        transport.bluez.run(&transport.cancel, CALL_TIMEOUT, async {
            let proxy = transport.bluez.proxy(&characteristic, CHARACTERISTIC).await?;
            transport.notifying = true;
            proxy.call::<_, _, ()>("StartNotify", &()).await.context("Cannot receive Bluetooth MIDI. If pairing is required, use Pair device and accept the desktop prompt")?;
            Ok(())
        })?;
        let connection = transport.bluez.connection.clone();
        let device_path = transport.device.clone();
        let lost = transport.lost.clone();
        transport.monitor = Some(transport.bluez.runtime.spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                let connected = tokio::time::timeout(CALL_TIMEOUT, async {
                    let proxy: Proxy<'_> = zbus::proxy::Builder::new(&connection)
                        .destination(BLUEZ)?
                        .path(device_path.clone())?
                        .interface(DEVICE)?
                        .cache_properties(zbus::proxy::CacheProperties::No)
                        .build()
                        .await?;
                    proxy.get_property::<bool>("Connected").await
                })
                .await;
                if !matches!(connected, Ok(Ok(true))) {
                    lost.store(true, Ordering::Relaxed);
                    break;
                }
            }
        }));
        Ok(transport)
    }
    pub fn is_connected(&self) -> bool {
        !self.lost.load(Ordering::Relaxed)
    }
    fn check(&self) -> Result<()> {
        ensure!(
            !self.cancel.load(Ordering::Relaxed),
            "Bluetooth operation cancelled"
        );
        ensure!(
            self.is_connected(),
            "Bluetooth MIDI connection lost or receive buffer overflowed. Connect again"
        );
        Ok(())
    }
}
impl Transport for BluetoothTransport {
    fn is_connected(&self) -> bool {
        BluetoothTransport::is_connected(self)
    }
    fn wait(&mut self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;
        while Instant::now() < deadline {
            self.check()?;
            thread::sleep(
                deadline
                    .saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(10)),
            );
        }
        self.check()
    }
    fn send(&mut self, message: &[u8]) -> Result<()> {
        self.check()?;
        let packets = codec::encode(
            message,
            (self.started.elapsed().as_millis() & 8191) as u16,
            PACKET_CAPACITY,
        )?;
        if let Some(delay) = self.next_send.checked_duration_since(Instant::now()) {
            self.wait(delay)?;
        }
        for packet in packets {
            self.check()?;
            let endpoint = self
                .endpoint
                .as_ref()
                .context("Bluetooth MIDI endpoint missing")?;
            let result = self.bluez.run(&self.cancel, CALL_TIMEOUT, async {
                let proxy = self.bluez.proxy(&endpoint.path, CHARACTERISTIC).await?;
                let options: HashMap<&str, Value<'_>> =
                    HashMap::from([("type", Value::from(endpoint.write_type))]);
                proxy
                    .call::<_, _, ()>("WriteValue", &(packet, options))
                    .await?;
                Ok(())
            });
            if result.is_err() {
                self.lost.store(true, Ordering::Relaxed);
            }
            result.context("Bluetooth MIDI send failed; no automatic retry was attempted")?;
            self.wait(Duration::from_millis(10))?;
        }
        self.next_send = Instant::now() + Duration::from_millis(20);
        Ok(())
    }
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            self.check()?;
            if let Some(frame) = self.frames.pop_front() {
                return Ok(Some(frame));
            }
            match self.notifications.try_recv() {
                Ok(Ok(packet)) => match self.decoder.feed(&packet) {
                    Ok(frames) => self.frames.extend(frames),
                    Err(error) => {
                        self.lost.store(true, Ordering::Relaxed);
                        return Err(error);
                    }
                },
                Ok(Err(error)) => bail!("Bluetooth notifications failed: {error}"),
                Err(TryRecvError::Disconnected) => bail!("Bluetooth notification stream closed"),
                Err(TryRecvError::Empty) => {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                    thread::sleep(Duration::from_millis(2));
                }
            }
        }
    }
}
impl Drop for BluetoothTransport {
    fn drop(&mut self) {
        if let Some(task) = self.watcher.take() {
            task.abort();
        }
        if let Some(task) = self.monitor.take() {
            task.abort();
        }
        self.bluez.runtime.block_on(async {
            if self.notifying
                && let Some(endpoint) = &self.endpoint
            {
                self.bluez
                    .cleanup_call(&endpoint.path, CHARACTERISTIC, "StopNotify")
                    .await;
            }
            if self.connected_by_us {
                self.bluez
                    .cleanup_call(&self.device, DEVICE, "Disconnect")
                    .await;
            }
        });
    }
}

#[cfg(test)]
mod tests;
