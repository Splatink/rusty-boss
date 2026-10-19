use rusty_boss::{
    document::{Backup, LiveSet, Patch},
    protocol::{Address, Decoder, Identity, Message},
    schema::{Schema, slot_address},
};
use std::{fs, path::PathBuf};
#[test]
fn every_recovered_numeric_parameter_round_trips_at_bounds() {
    let schema = Schema::load();
    for block in &schema.blocks {
        for p in &block.parameters {
            if p.encoding == "ascii" {
                continue;
            }
            for value in [p.min, p.max, p.initial.as_i64().unwrap() as i32] {
                let encoded = p.encode_number(value).unwrap();
                let mut data = vec![0; block.size];
                data[p.offset..p.offset + p.width].copy_from_slice(&encoded);
                assert_eq!(p.decode_number(&data).unwrap(), value, "{}", p.name);
            }
            assert!(p.encode_number(p.min - 1).is_err());
            assert!(p.encode_number(p.max + 1).is_err());
        }
    }
}
#[test]
fn address_space_and_packet_round_trips() {
    for packed in [
        0, 0x0000007f, 0x00000100, 0x20000000, 0x7f000104, 0x7f7f7f7f,
    ] {
        let addr = Address::packed(packed).unwrap();
        assert_eq!(u32::from_be_bytes(addr.bytes()), packed);
        if packed == 0x7f7f7f7f {
            assert!(addr.plus(1).is_err());
        }
        for size in [1, 16, 128] {
            if addr.plus(size - 1).is_err() {
                continue;
            }
            let msg = Message::Data {
                device: 16,
                address: addr,
                data: vec![127; size],
            };
            let bytes = msg.encode().unwrap();
            assert_eq!(Message::decode(&bytes).unwrap(), msg);
            let mut decoder = Decoder::default();
            let mut frames = Vec::new();
            for b in bytes {
                frames.extend(decoder.feed(&[b]));
            }
            assert_eq!(frames.len(), 1);
            assert_eq!(Message::decode(&frames[0]).unwrap(), msg);
        }
    }
}
#[test]
fn all_model_codes_and_slot_boundaries() {
    for code in 5..=11 {
        let reply = [
            0xf0, 0x7e, 16, 6, 2, 0x41, 7, 5, 0, 0, code, 0, 0, 0, 0, 0xf7,
        ];
        let id = Identity::decode(&reply).unwrap();
        assert!(id.slot_exists(0));
        assert!(!id.slot_exists(9));
        assert_eq!(id.slot_exists(3), code != 5);
    }
    for slot in 0..9 {
        assert!(slot_address(slot).is_ok());
    }
    assert!(slot_address(9).is_err());
}
#[test]
fn tsl_preserves_future_fields_and_hex_data() {
    let s = Schema::load();
    let mut set = LiveSet::new("Future");
    let mut p = Patch::new(&s).unwrap();
    p.extra
        .insert("futurePatchField".into(), serde_json::json!({"v":1}));
    p.parameters.insert("FUTURE%BLOCK".into(), vec![0, 127]);
    set.extra
        .insert("futureDocumentField".into(), serde_json::json!([1, 2]));
    set.data[0].push(p);
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("future.tsl");
    set.save(&path).unwrap();
    let (loaded, w) = LiveSet::load(&path, &s).unwrap();
    assert!(w.is_empty());
    assert_eq!(
        serde_json::to_value(set).unwrap(),
        serde_json::to_value(loaded).unwrap()
    );
}
#[test]
fn rejects_wrong_product_revision_and_oversized_input() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.tsl");
    let s = Schema::load();
    fs::write(
        &path,
        r#"{"name":"bad","formatRev":"0000","device":"KATANA MkII","data":[[]]}"#,
    )
    .unwrap();
    assert!(LiveSet::load(&path, &s).is_err());
    fs::write(
        &path,
        r#"{"name":"bad","formatRev":"9999","device":"KATANA Gen3","data":[[]]}"#,
    )
    .unwrap();
    assert!(LiveSet::load(&path, &s).is_err());
    fs::write(&path, vec![b' '; 16 * 1024 * 1024 + 1]).unwrap();
    assert!(LiveSet::load(&path, &s).is_err());
    assert!(Backup::load(&PathBuf::from("/nonexistent/rusty-boss-backup.alb")).is_err());
}
#[test]
fn no_raw_write_can_cross_address_space() {
    assert!(
        Message::Data {
            device: 16,
            address: Address::packed(0x7f7f7f7f).unwrap(),
            data: vec![1, 2]
        }
        .encode()
        .is_err()
    );
    assert!(
        Message::Data {
            device: 128,
            address: Address::new(0).unwrap(),
            data: vec![1]
        }
        .encode()
        .is_err()
    );
    assert!(
        Message::Data {
            device: 16,
            address: Address::new(0).unwrap(),
            data: vec![128]
        }
        .encode()
        .is_err()
    );
}
