// Copyright 2026 AsterSQL.
//! Derive scatter continuity groups from memcomparable region keys.
use astersql_pd_codec::{
    KEYSPACE_PREFIX_LEN, TXN_KEYSPACE_MODE_PREFIX, decode_bytes, decode_int, encode_bytes,
    generate_index_key, generate_table_key, parse_keyspace_prefix,
};
use astersql_pd_core::RegionInfo;

/// Derived table/index range. Empty fields mean no recognizable group;
/// an empty end key on a nonempty hint means an unbounded prefix range.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SplitScatterRangeHint {
    pub start_key: Vec<u8>,
    pub end_key: Vec<u8>,
    pub scatter_group: String,
}

/// Resolve the start key's table or index group, preserving Go's table fallback.
///
/// A transactional keyspace prefix is stripped only when `validate_keyspace`
/// confirms its known bounds. Without validation it is ambiguous user data.
/// Malformed start keys yield an empty hint; end keys are compared as encoded
/// bytes, without decoding or requiring a well-formed region range.
pub fn resolve_split_scatter_range_hint(
    region: &RegionInfo,
    validate_keyspace: Option<&dyn Fn(u32) -> bool>,
) -> SplitScatterRangeHint {
    let range = &region.meta().key_range;
    let Ok((_, decoded)) = decode_bytes(&range.start_key) else {
        return SplitScatterRangeHint::default();
    };
    let mut raw = decoded.as_slice();
    let mut keyspace_prefix = &[][..];
    let mut keyspace_id = None;
    if let Ok((TXN_KEYSPACE_MODE_PREFIX, id)) = parse_keyspace_prefix(raw)
        && validate_keyspace.is_some_and(|validate| validate(id))
    {
        keyspace_prefix = &raw[..KEYSPACE_PREFIX_LEN];
        raw = &raw[KEYSPACE_PREFIX_LEN..];
        keyspace_id = Some(id);
    }
    let Some(rest) = raw.strip_prefix(b"t") else {
        return SplitScatterRangeHint::default();
    };
    let Ok((rest, table_id)) = decode_int(rest) else {
        return SplitScatterRangeHint::default();
    };
    let table_group = match keyspace_id {
        Some(id) => format!("split-scatter-keyspace-{id}-table-{table_id}"),
        None => format!("split-scatter-table-{table_id}"),
    };
    let table_range = || {
        split_scatter_prefix_range_with_keyspace_group(
            keyspace_prefix,
            &generate_table_key(table_id),
            &table_group,
        )
    };
    let Some(index_rest) = rest.strip_prefix(b"_i") else {
        return table_range();
    };
    let Ok((_, index_id)) = decode_int(index_rest) else {
        return SplitScatterRangeHint::default();
    };
    let index_group = match keyspace_id {
        Some(id) => format!("split-scatter-keyspace-{id}-index-{table_id}-{index_id}"),
        None => format!("split-scatter-index-{table_id}-{index_id}"),
    };
    let index_range = split_scatter_prefix_range_with_keyspace_group(
        keyspace_prefix,
        &generate_index_key(table_id, index_id),
        &index_group,
    );
    if range.end_key.is_empty()
        || index_range.start_key.is_empty()
        || index_range.end_key.is_empty()
        || range.end_key > index_range.end_key
    {
        return table_range();
    }
    index_range
}

/// Build memcomparable prefix bounds, incrementing only the raw entity prefix.
/// An empty/all-0xff raw prefix has no finite successor, even in a keyspace.
pub fn split_scatter_prefix_range_with_keyspace_group(
    keyspace_prefix: &[u8],
    raw_prefix: &[u8],
    scatter_group: &str,
) -> SplitScatterRangeHint {
    let encode = |raw: &[u8]| {
        let mut key = Vec::with_capacity(keyspace_prefix.len() + raw.len());
        key.extend_from_slice(keyspace_prefix);
        key.extend_from_slice(raw);
        encode_bytes(&key)
    };
    let next = split_scatter_next_prefix(raw_prefix);
    SplitScatterRangeHint {
        start_key: encode(raw_prefix),
        end_key: if next.is_empty() {
            Vec::new()
        } else {
            encode(&next)
        },
        scatter_group: scatter_group.into(),
    }
}

/// Return the shortest exclusive prefix successor, or empty for infinity.
/// The input is never modified.
pub fn split_scatter_next_prefix(key: &[u8]) -> Vec<u8> {
    let Some(index) = key.iter().rposition(|&byte| byte != 0xff) else {
        return Vec::new();
    };
    let mut next = key[..=index].to_vec();
    next[index] += 1;
    next
}
