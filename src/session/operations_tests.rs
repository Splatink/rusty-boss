//! Memory transport only: no ALSA, ports or hardware.
use super::*;

#[derive(Default)]
struct Memory {
    bytes: BTreeMap<u32, u8>,
    incoming: VecDeque<Vec<u8>>,
    sent: Vec<Message>,
    waits: Vec<Duration>,
    fail_at: Option<usize>,
    drop_export_ack: bool,
    cancel_wait: bool,
}
impl Transport for Memory {
    fn send(&mut self, frame: &[u8]) -> Result<()> {
        ensure!(self.fail_at != Some(self.sent.len()), "injected failure");
        let m = Message::decode(frame)?;
        let reply = match &m {
            Message::Request {
                device,
                address,
                size,
            } => Some(Message::Data {
                device: *device,
                address: *address,
                data: (0..*size)
                    .map(|i| *self.bytes.get(&(address.linear() + i)).unwrap_or(&0))
                    .collect(),
            }),
            Message::Data {
                device,
                address,
                data,
            } => {
                for (i, b) in data.iter().enumerate() {
                    self.bytes.insert(address.linear() + i as u32, *b);
                }
                if *address == Address::packed(0x7f000100)? {
                    self.bytes.insert(0, data[0]);
                    self.bytes.insert(1, data[1]);
                }
                if [0x7f000104, 0x7f01010a]
                    .into_iter()
                    .any(|a| *address == Address::packed(a).unwrap())
                    && !(self.drop_export_ack && *address == Address::packed(0x7f01010a)?)
                {
                    Some(Message::Data {
                        device: *device,
                        address: *address,
                        data: vec![0],
                    })
                } else {
                    None
                }
            }
        };
        self.sent.push(m);
        if let Some(reply) = reply {
            self.incoming.push_back(reply.encode()?);
        }
        Ok(())
    }
    fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
        Ok(self.incoming.pop_front())
    }
    fn wait(&mut self, duration: Duration) -> Result<()> {
        self.waits.push(duration);
        ensure!(!self.cancel_wait, "injected cancellation");
        Ok(())
    }
}
fn session() -> Session<Memory> {
    let mut transport = Memory::default();
    for b in Schema::load()
        .blocks
        .iter()
        .filter(|b| b.id.starts_with("SYSTEM%"))
    {
        for (i, byte) in b.defaults().unwrap().iter().enumerate() {
            transport.bytes.insert(b.address + i as u32, *byte);
        }
    }
    Session {
        transport,
        identity: Identity {
            device: 16,
            model_code: 6,
            name: "KATANA-100",
        },
        timeout: Duration::from_millis(2),
        active: false,
        updates: VecDeque::new(),
    }
}
fn backup(s: &Schema) -> Backup {
    Backup {
        device: DEVICE.into(),
        revision: "0000".into(),
        model: "KATANA-100".into(),
        system: s.defaults("SYSTEM").unwrap(),
        setup: s.defaults("SETUP").unwrap(),
        patches: (0..9)
            .map(|_| Some(BackupPatch(s.defaults("PATCH").unwrap())))
            .collect(),
        extra: BTreeMap::new(),
    }
}
#[test]
fn system_changes_are_validated_checked_and_read_back() {
    let schema = Schema::load();
    let before = schema.defaults("SYSTEM").unwrap();
    let mut after = before.clone();
    after.get_mut("SYSTEM%USB_COM").unwrap()[2] = 73;
    let mut s = session();
    let result = s.apply_system(&schema, &before, &after).unwrap();
    assert_eq!(result, after);
    assert_eq!(
        s.transport
            .sent
            .iter()
            .filter(|m| matches!(m, Message::Data { .. }))
            .count(),
        1
    );
    let mut s = session();
    let addr = schema.block("SYSTEM%USB_COM").unwrap().address + 2;
    s.transport.bytes.insert(addr, 65);
    assert!(s.apply_system(&schema, &before, &after).is_err());
    assert!(s
        .transport
        .sent
        .iter()
        .all(|m| matches!(m, Message::Request { .. })));
    let mut s = session();
    after.get_mut("SYSTEM%USB_COM").unwrap()[2] = 127;
    assert!(s.apply_system(&schema, &before, &after).is_err());
    assert!(s.transport.sent.is_empty());
}
#[test]
fn bulk_save_preflights_every_tone_and_stops_on_failure() {
    let schema = Schema::load();
    let patch = Patch::new(&schema).unwrap();
    let mut bad = patch.clone();
    bad.parameters.get_mut("PATCH%AMP").unwrap().clear();
    let mut s = session();
    assert!(s
        .store_patches(&schema, &[(1, patch.clone()), (2, bad)], |_, _| {})
        .is_err());
    assert!(s.transport.sent.is_empty());
    assert!(s
        .store_patches(
            &schema,
            &[(1, patch.clone()), (1, patch.clone())],
            |_, _| {}
        )
        .is_err());
    assert!(s.transport.sent.is_empty());
    let mut progress = vec![];
    s.store_patches(
        &schema,
        &[(1, patch.clone()), (2, patch.clone())],
        |i, n| progress.push((i, n)),
    )
    .unwrap();
    assert_eq!(progress, [(1, 2), (2, 2)]);
    let mut s = session();
    s.transport.fail_at = Some(1);
    assert!(s
        .store_patches(&schema, &[(1, patch.clone()), (2, patch)], |_, _| {})
        .is_err());
    assert!(!s.transport.sent.iter().any(|m| matches!(m, Message::Data { address, .. } if *address == Address::packed(0x7f000104).unwrap())));
}
#[test]
fn restore_preflight_is_all_or_nothing_and_never_accepts_other_models() {
    let schema = Schema::load();
    for variant in 0..6 {
        let mut b = backup(&schema);
        match variant {
            0 => b.model = "KATANA-50".into(),
            1 => b.patches[8] = None,
            2 => {
                b.system.remove("SYSTEM%USB_COM");
            }
            3 => b.setup.get_mut("SETUP%COM").unwrap()[1] = 9,
            4 => {
                b.patches[8]
                    .as_mut()
                    .unwrap()
                    .0
                    .get_mut("PATCH%AMP")
                    .unwrap()[0] = 127
            }
            _ => b
                .patches
                .push(Some(BackupPatch(schema.defaults("PATCH").unwrap()))),
        }
        let mut s = session();
        assert!(
            s.restore_backup(&schema, &b, |_, _| {}).is_err(),
            "variant {variant}"
        );
        assert!(s.transport.sent.is_empty());
    }
}
#[test]
fn restore_stages_exact_roots_then_waits_and_exports_once() {
    let schema = Schema::load();
    let mut s = session();
    let mut b = backup(&schema);
    b.setup.get_mut("SETUP%COM").unwrap()[1] = 2;
    let mut progress = vec![];
    s.restore_backup(&schema, &b, |i, n| progress.push((i, n)))
        .unwrap();
    assert_eq!(s.transport.waits, [Duration::from_secs(10)]);
    assert!(progress.windows(2).all(|w| w[1].0 == w[0].0 + 1));
    assert_eq!(progress.last().unwrap().0, progress.last().unwrap().1);
    let exports: Vec<_> = s.transport.sent.iter().filter(|m| matches!(m, Message::Data { address, .. } if *address == Address::packed(0x7f01010a).unwrap())).collect();
    assert!(matches!(&exports[..], [Message::Data { data, .. }] if data == &[127,127]));
    for slot in 0..9 {
        assert!(s.transport.sent.iter().any(|m| matches!(m, Message::Data { address, .. } if *address == slot_address(slot).unwrap())));
    }
    assert!(!s
        .transport
        .sent
        .iter()
        .any(|m| matches!(m, Message::Data { address, .. } if address.linear() == 0)));
    assert_eq!(s.transport.bytes[&1], 2);
}
#[test]
fn restore_failures_never_retry_export_or_select_a_channel() {
    let schema = Schema::load();
    for failure in 0..3 {
        let mut s = session();
        match failure {
            0 => s.transport.fail_at = Some(1),
            1 => s.transport.cancel_wait = true,
            _ => s.transport.drop_export_ack = true,
        }
        assert!(s
            .restore_backup(&schema, &backup(&schema), |_, _| {})
            .is_err());
        let exports = s.transport.sent.iter().filter(|m| matches!(m, Message::Data { address, .. } if *address == Address::packed(0x7f01010a).unwrap())).count();
        assert_eq!(exports, usize::from(failure == 2));
        assert!(!s.transport.sent.iter().any(|m| matches!(m, Message::Data { address, .. } if *address == Address::packed(0x7f000100).unwrap())));
        if failure == 2 {
            assert!(
                matches!(s.transport.sent.last().unwrap(), Message::Data { address,data,.. } if *address == Address::packed(0x7f010109).unwrap() && data == &[0])
            );
        }
    }
}
#[test]
fn macros_and_taps_use_only_the_narrow_firmware_commands() {
    let schema = Schema::load();
    let mut s = session();
    s.panel_macro(&schema, 4, 52).unwrap();
    s.tap(1).unwrap();
    assert!(
        matches!(&s.transport.sent[0], Message::Data { address, data, .. } if *address == Address::packed(0x30000204).unwrap() && data == &[53])
    );
    assert!(
        matches!(&s.transport.sent[1], Message::Data { address, data, .. } if *address == Address::packed(0x7f010107).unwrap() && data == &[0])
    );
    for (index, value) in [(6, 10), (0, -1), (0, 101), (5, 50)] {
        assert!(s.panel_macro(&schema, index, value).is_err());
    }
    assert!(s.tap(2).is_err());
    assert!(s.tap(3).is_err());
    assert_eq!(s.transport.sent.len(), 2);
}
#[test]
fn reads_and_command_acknowledgements_preserve_unsolicited_updates() {
    let mut s = session();
    let notification = Message::Data {
        device: 16,
        address: Address::packed(0x20000600).unwrap(),
        data: vec![42],
    };
    s.transport
        .incoming
        .push_back(notification.encode().unwrap());
    s.read(Address::packed(0).unwrap(), 2).unwrap();
    assert_eq!(
        s.poll().unwrap().unwrap().encode().unwrap(),
        notification.encode().unwrap()
    );
    s.transport
        .incoming
        .push_back(notification.encode().unwrap());
    s.command_ack(Address::packed(0x7f000104).unwrap(), vec![0, 1])
        .unwrap();
    assert_eq!(
        s.poll().unwrap().unwrap().encode().unwrap(),
        notification.encode().unwrap()
    );
}
