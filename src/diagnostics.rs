//! Bounded, strictly read-only reverb snapshot. No editor setup or cleanup writes.
use crate::{
    document::ParamSet,
    editor::{Catalog, Section},
    schema::Schema,
    session::{Session, Transport},
};
use anyhow::Result;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    rc::Rc,
    time::{Duration, Instant},
};

struct Recording<T> {
    inner: T,
    frames: Rc<RefCell<Vec<Value>>>,
    start: Instant,
    omitted: Rc<RefCell<usize>>,
}
impl<T> Recording<T> {
    fn record(&self, direction: &str, bytes: &[u8]) {
        let mut frames = self.frames.borrow_mut();
        if frames.len() < 512 {
            frames.push(json!({"direction": direction, "elapsed_ms": self.start.elapsed().as_millis() as u64, "bytes": bytes}));
        } else {
            *self.omitted.borrow_mut() += 1;
        }
    }
}
impl<T: Transport> Transport for Recording<T> {
    fn send(&mut self, bytes: &[u8]) -> Result<()> {
        self.inner.send(bytes)?;
        self.record("tx", bytes);
        Ok(())
    }
    fn receive(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>> {
        let frame = self.inner.receive(timeout)?;
        if let Some(bytes) = &frame {
            self.record("rx", bytes);
        }
        Ok(frame)
    }
}

/// Returns an auditable report, including partial results on timeout/error.
/// Exactly one identity request and at most eight RQ1s, with no retries. Never
/// selects a channel, enables editor mode, or substitutes patch defaults.
pub fn reverb_snapshot<T: Transport>(transport: T, schema: &Schema) -> Value {
    let frames = Rc::new(RefCell::new(Vec::new()));
    let omitted = Rc::new(RefCell::new(0));
    let recording = Recording {
        inner: transport,
        frames: frames.clone(),
        start: Instant::now(),
        omitted: omitted.clone(),
    };
    let mut set = ParamSet::new();
    let mut identity = Value::Null;
    let mut bookends = ParamSet::new();
    let result = (|| -> Result<()> {
        let mut session = Session::observe(recording)?;
        identity = json!({"name": session.identity.name, "model_code": session.identity.model_code, "device_id": session.identity.device});
        for id in [
            "PATCH%COLOR",
            "PATCH%ASSIGN_KNOBS",
            "PATCH%PATCH_KNOB_READONLY",
            "PATCH%REVERB(1)",
            "PATCH%REVERB(2)",
            "PATCH%REVERB(3)",
        ] {
            let b = schema.block(id)?;
            set.insert(id.into(), session.read(b.address(), b.size)?);
        }
        for id in ["PATCH%COLOR", "PATCH%PATCH_KNOB_READONLY"] {
            let b = schema.block(id)?;
            bookends.insert(id.into(), session.read(b.address(), b.size)?);
        }
        session.close()?; // observe() never sets active; emits no cleanup packet.
        Ok(())
    })();
    let catalog = Catalog::load();
    let banks = (1..=3).map(|bank| {
        let level = catalog.control("reverb-reverb-effect-level-dial").unwrap().remap("REVERB(1)", &format!("REVERB({bank})"));
        let algorithm = catalog.control("reverb-type-select-box").unwrap().remap("REVERB(1)", &format!("REVERB({bank})"));
        json!({"bank": (["green", "red", "yellow"][bank - 1]), "effect_level": level.value(schema, &set).ok(), "algorithm": algorithm.value(schema, &set).ok().map(|v| algorithm.text(v))})
    }).collect::<Vec<_>>();
    json!({
        "format": "rusty-boss/read-only-reverb/1",
        "policy": "identity and RQ1 only; all DT1 blocked; no editor-mode setup/cleanup; no retries",
        "identity": identity,
        "error": result.err().map(|e| e.to_string()),
        "panel_position": catalog.panel_position(schema, &set, Section::Reverb),
        "reverb_color": set.get("PATCH%COLOR").and_then(|d| d.get(4)).map(|v| ["green", "red", "yellow"].get(*v as usize).copied().unwrap_or("unknown")),
        "knob_assignment_raw": set.get("PATCH%ASSIGN_KNOBS").and_then(|d| d.get(2)),
        "banks": banks,
        "blocks": set,
        "bookend_blocks": bookends,
        "bookends_match": bookends.len() == 2 && bookends.iter().all(|(id, data)| set.get(id) == Some(data)),
        "atomic_snapshot": false,
        "frames": *frames.borrow(),
        "omitted_frames": *omitted.borrow()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Address, IDENTITY_REQUEST, Message};
    use std::collections::VecDeque;
    struct Mock {
        replies: VecDeque<Vec<u8>>,
    }
    impl Transport for Mock {
        fn send(&mut self, bytes: &[u8]) -> Result<()> {
            assert!(
                bytes == IDENTITY_REQUEST
                    || matches!(Message::decode(bytes)?, Message::Request { .. })
            );
            Ok(())
        }
        fn receive(&mut self, _: Duration) -> Result<Option<Vec<u8>>> {
            Ok(self.replies.pop_front())
        }
    }
    #[test]
    fn probe_records_only_identity_and_eight_reads_and_no_drop_write() {
        let schema = Schema::load();
        let mut set = schema.defaults("PATCH").unwrap();
        set.get_mut("PATCH%PATCH_KNOB_READONLY").unwrap()[4] = 51;
        set.get_mut("PATCH%REVERB(1)").unwrap()[10] = 68;
        let mut replies = VecDeque::from([vec![
            0xf0, 0x7e, 0x10, 6, 2, 0x41, 7, 5, 0, 0, 6, 0, 0, 0, 0, 0xf7,
        ]]);
        for id in [
            "PATCH%COLOR",
            "PATCH%ASSIGN_KNOBS",
            "PATCH%PATCH_KNOB_READONLY",
            "PATCH%REVERB(1)",
            "PATCH%REVERB(2)",
            "PATCH%REVERB(3)",
            "PATCH%COLOR",
            "PATCH%PATCH_KNOB_READONLY",
        ] {
            replies.push_back(
                Message::Data {
                    device: 16,
                    address: schema.block(id).unwrap().address(),
                    data: set[id].clone(),
                }
                .encode()
                .unwrap(),
            );
        }
        let report = reverb_snapshot(Mock { replies }, &schema);
        assert!(report["error"].is_null());
        assert_eq!(report["panel_position"], 50);
        assert_eq!(report["banks"][0]["effect_level"], 68);
        assert_eq!(report["bookends_match"], true);
        let frames = report["frames"].as_array().unwrap();
        assert_eq!(frames.iter().filter(|f| f["direction"] == "tx").count(), 9);
        assert_eq!(frames.len(), 18);
        // Assert exact current-patch addresses; no command-area reads/writes.
        for frame in frames.iter().filter(|f| f["direction"] == "tx").skip(1) {
            let data = serde_json::from_value::<Vec<u8>>(frame["bytes"].clone()).unwrap();
            if let Message::Request { address, .. } = Message::decode(&data).unwrap() {
                assert!(address.linear() >= Address::packed(0x20000000).unwrap().linear());
                assert!(address.linear() < Address::packed(0x20100000).unwrap().linear());
            } else {
                panic!("write escaped read-only probe")
            }
        }
    }
}
