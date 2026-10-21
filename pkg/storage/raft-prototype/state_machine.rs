// Copyright 2026 AsterSQL.
use std::collections::BTreeMap;

/// Single-key CAS experiment; leases and historical reads deliberately unsupported.
#[derive(Default, Debug, PartialEq, Eq)]
pub struct State {
    pub revision: u64,
    pub values: BTreeMap<String, (String, u64)>,
    pub events: Vec<(u64, String, String)>,
}
impl State {
    pub fn apply(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        let (key, value, expected): (String, String, u64) = serde_json::from_slice(data).unwrap();
        let current = self.values.get(&key).map_or(0, |v| v.1);
        if current != expected {
            return;
        }
        self.revision += 1;
        self.values
            .insert(key.clone(), (value.clone(), self.revision));
        self.events.push((self.revision, key, value));
    }
    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(&(self.revision, &self.values, &self.events)).unwrap()
    }
    pub fn decode(bytes: &[u8]) -> Self {
        let (revision, values, events) = serde_json::from_slice(bytes).unwrap();
        Self {
            revision,
            values,
            events,
        }
    }
}
