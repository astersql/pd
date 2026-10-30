// Copyright 2026 AsterSQL.
//! Go's bounded min-heap selection over immutable canonical Region snapshots.
use astersql_pd_core::RegionInfo;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(crate) const DIMENSIONS: [&str; 9] = [
    "writeflow",
    "writequery",
    "readflow",
    "readquery",
    "confver",
    "version",
    "size",
    "keys",
    "cpu",
];
fn value(region: &RegionInfo, dimension: &str) -> i128 {
    match dimension {
        "writeflow" => region.stats().written_bytes.into(),
        "readflow" => region.stats().read_bytes.into(),
        "writequery" => region.write_query_num().into(),
        "readquery" => region.read_query_num().into(),
        "confver" => region
            .meta()
            .epoch
            .as_ref()
            .map_or(0, |e| e.conf_ver)
            .into(),
        "version" => region.meta().epoch.as_ref().map_or(0, |e| e.version).into(),
        "size" => region.stats().approximate_size.into(),
        "keys" => region.stats().approximate_keys.into(),
        "cpu" => region.response_cpu_usage().into(),
        _ => unreachable!("registered top dimension"),
    }
}
type Entry = (i128, Arc<RegionInfo>);
fn push(heap: &mut Vec<Entry>, item: Entry) {
    heap.push(item);
    let mut child = heap.len() - 1;
    while child > 0 {
        let parent = (child - 1) / 2;
        if heap[child].0 >= heap[parent].0 {
            break;
        }
        heap.swap(child, parent);
        child = parent;
    }
}
fn pop(heap: &mut Vec<Entry>) -> Entry {
    let last = heap.len() - 1;
    heap.swap(0, last);
    let result = heap.pop().expect("nonempty heap");
    let mut parent = 0;
    loop {
        let mut child = parent * 2 + 1;
        if child >= heap.len() {
            break;
        }
        if child + 1 < heap.len() && heap[child + 1].0 < heap[child].0 {
            child += 1;
        }
        if heap[child].0 >= heap[parent].0 {
            break;
        }
        heap.swap(parent, child);
        parent = child;
    }
    result
}
/// Equal values never replace the heap minimum. There is no ID tie-breaker in
/// Go; identical input order follows container/heap's left-child tie behavior.
/// i128 preserves both uint64 counters and signed approximate-key values.
pub(crate) async fn select(
    regions: Vec<Arc<RegionInfo>>,
    dimension: &str,
    limit: i64,
    cancel: &CancellationToken,
) -> Result<Vec<Arc<RegionInfo>>, &'static str> {
    if limit <= 0 {
        return Ok(vec![]);
    }
    let limit = (limit.min(10240) as usize).min(regions.len());
    let mut heap = Vec::with_capacity(limit);
    for (index, region) in regions.into_iter().enumerate() {
        if index % 256 == 0 {
            tokio::task::yield_now().await;
        }
        if cancel.is_cancelled() {
            return Err("context canceled");
        }
        let item = (value(&region, dimension), region);
        if heap.len() < limit {
            push(&mut heap, item);
        } else if item.0 > heap[0].0 {
            pop(&mut heap);
            push(&mut heap, item);
        }
    }
    let mut result = Vec::with_capacity(heap.len());
    while !heap.is_empty() {
        if cancel.is_cancelled() {
            return Err("context canceled");
        }
        result.push(pop(&mut heap).1);
    }
    result.reverse();
    Ok(result)
}
