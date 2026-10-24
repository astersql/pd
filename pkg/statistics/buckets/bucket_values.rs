// Copyright 2026 AsterSQL.

/// Per-bucket counters in read bytes/keys/queries, write bytes/keys/queries order.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BucketFlows {
    pub read_bytes: Vec<u64>,
    pub read_keys: Vec<u64>,
    pub read_qps: Vec<u64>,
    pub write_bytes: Vec<u64>,
    pub write_keys: Vec<u64>,
    pub write_qps: Vec<u64>,
}

/// Protocol-independent report; period is in milliseconds and keys are ordered.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BucketReport {
    pub region_id: u64,
    pub version: u64,
    pub keys: Vec<Vec<u8>>,
    pub period_in_ms: u64,
    pub flows: BucketFlows,
}

/// Owned per-bucket snapshot, with an empty end key denoting infinity.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BucketStat {
    pub region_id: u64,
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub hot_degree: i32,
    pub interval: u64,
    pub loads: [u64; 6],
}

/// Lifecycle of an indexed report; fragments are archived by range operations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BucketStatus {
    #[default]
    Alive,
    Archive,
}

/// Owned region report snapshot; Clone isolates all keys, stats and loads.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BucketTreeItem {
    pub region_id: u64,
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub stats: Vec<BucketStat>,
    pub interval: u64,
    pub version: u64,
    pub status: BucketStatus,
}

/// Normalizes counters to integer rates per second.
///
/// Panics for an empty key array or counters shorter than the bucket count,
/// preserving the source's indexing boundary. Extra counters are ignored.
pub fn convert_to_bucket_tree_item(report: &BucketReport) -> BucketTreeItem {
    assert!(
        !report.keys.is_empty(),
        "bucket report requires at least one key"
    );
    let interval = if report.period_in_ms == 0 {
        10_000
    } else {
        report.period_in_ms
    };
    let flows = &report.flows;
    let counters = [
        &flows.read_bytes,
        &flows.read_keys,
        &flows.read_qps,
        &flows.write_bytes,
        &flows.write_keys,
        &flows.write_qps,
    ];
    let stats = report
        .keys
        .windows(2)
        .enumerate()
        .map(|(i, keys)| BucketStat {
            region_id: report.region_id,
            start_key: keys[0].clone(),
            end_key: keys[1].clone(),
            hot_degree: 0,
            interval,
            loads: counters.map(|counter| counter[i].wrapping_mul(1000) / interval),
        })
        .collect();
    BucketTreeItem {
        region_id: report.region_id,
        start_key: report.keys[0].clone(),
        end_key: report.keys.last().expect("nonempty keys").clone(),
        stats,
        interval,
        version: report.version,
        status: BucketStatus::Alive,
    }
}
