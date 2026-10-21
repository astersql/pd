// Copyright 2026 AsterSQL.
//! Full-image journal: intentionally slow but with an explicit persistence boundary.
use protobuf::Message;
use raft::{Storage, prelude::*, storage::MemStorage};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{self, Write},
    path::Path,
};

pub fn save(path: &Path, store: &MemStorage, snapshot: Snapshot) -> io::Result<()> {
    let state = store.initial_state().unwrap();
    let first = store
        .first_index()
        .unwrap()
        .max(snapshot.get_metadata().index + 1);
    let entries = if first > store.last_index().unwrap() {
        vec![]
    } else {
        store
            .entries(
                first,
                store.last_index().unwrap() + 1,
                None,
                raft::GetEntriesContext::empty(false),
            )
            .unwrap()
    };
    let payload = serde_json::to_vec(&(
        snapshot.write_to_bytes().unwrap(),
        state.hard_state.write_to_bytes().unwrap(),
        entries
            .iter()
            .map(|e| e.write_to_bytes().unwrap())
            .collect::<Vec<_>>(),
    ))?;
    let mut file = File::create(path.with_extension("pending"))?;
    file.write_all(&Sha256::digest(&payload))?;
    file.write_all(&payload)?;
    file.sync_all()?;
    fs::rename(path.with_extension("pending"), path)?;
    File::open(path.parent().unwrap())?.sync_all()
}
pub fn load(path: &Path) -> io::Result<(MemStorage, Snapshot)> {
    if !path.exists() {
        let store = MemStorage::new_with_conf_state((vec![1, 2, 3], vec![]));
        let mut snapshot = Snapshot::default();
        snapshot
            .mut_metadata()
            .set_conf_state(store.initial_state().unwrap().conf_state);
        return Ok((store, snapshot));
    }
    let bytes = fs::read(path)?;
    if bytes.len() < 32 || Sha256::digest(&bytes[32..])[..] != bytes[..32] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "journal checksum mismatch",
        ));
    }
    let (snapshot, hard, entries): (Vec<u8>, Vec<u8>, Vec<Vec<u8>>) =
        serde_json::from_slice(&bytes[32..])?;
    let store = MemStorage::new_with_conf_state((vec![1, 2, 3], vec![]));
    let snapshot = Snapshot::parse_from_bytes(&snapshot)?;
    let mut core = store.wl();
    if snapshot.get_metadata().index > 0 {
        core.apply_snapshot(snapshot.clone()).unwrap();
    }
    let entries: Vec<Entry> = entries
        .iter()
        .map(|e| Entry::parse_from_bytes(e).unwrap())
        .collect();
    core.append(&entries).unwrap();
    core.set_hardstate(HardState::parse_from_bytes(&hard)?);
    drop(core);
    Ok((store, snapshot))
}
