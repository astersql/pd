// Copyright 2026 AsterSQL.
use crate::{EndpointError, StorageEndpoint, store_meta::decode_store};
use astersql_pd_core::{NodeState, StoreInfo, StoreMeta, StoreState};
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage_kv::{Base, Consistency, Read};

const PAGE_SIZE: usize = 100;
impl StorageEndpoint {
    /// Load stores in ID order, applying Go's lifecycle and weight defaults.
    /// Delivery is incremental: errors retain earlier callbacks. MAX is exclusive.
    pub async fn load_stores(
        &self,
        paths: &ClusterPaths,
        mut put_store: impl FnMut(StoreInfo),
    ) -> Result<(), EndpointError> {
        let mut next_id = 0;
        let end = paths.store_path(u64::MAX);
        loop {
            let start = paths.store_path(next_id);
            let mut request =
                Read::range(start.as_bytes(), end.as_bytes(), Consistency::Linearizable);
            request.limit = PAGE_SIZE;
            let page = self.store.read(request).await?;
            for entry in &page.entries {
                let mut meta: StoreMeta = decode_store(&entry.value)?;
                match meta.state {
                    StoreState::Offline => meta.node_state = NodeState::Removing,
                    StoreState::Tombstone => meta.node_state = NodeState::Removed,
                    _ => {}
                }
                let leader = self
                    .load_weight(&paths.store_leader_weight_path(meta.id))
                    .await?;
                let region = self
                    .load_weight(&paths.store_region_weight_path(meta.id))
                    .await?;
                next_id = meta.id.wrapping_add(1);
                let mut info = StoreInfo::new(meta);
                info.set_leader_weight(leader);
                info.set_region_weight(region);
                put_store(info);
            }
            if page.entries.len() < PAGE_SIZE {
                return Ok(());
            }
        }
    }
    /// Save leader then region in separate writes, preserving partial success on error.
    pub async fn save_store_weight(
        &self,
        paths: &ClusterPaths,
        id: u64,
        leader: f64,
        region: f64,
    ) -> Result<(), EndpointError> {
        let base = Base::new(self.store.as_ref());
        base.save(
            paths.store_leader_weight_path(id).as_bytes(),
            format_weight(leader).as_bytes(),
        )
        .await?;
        base.save(
            paths.store_region_weight_path(id).as_bytes(),
            format_weight(region).as_bytes(),
        )
        .await?;
        Ok(())
    }
    async fn load_weight(&self, key: &str) -> Result<f64, EndpointError> {
        let bytes = Base::new(self.store.as_ref()).load(key.as_bytes()).await?;
        if bytes.is_empty() {
            return Ok(1.0);
        }
        let text = std::str::from_utf8(&bytes).map_err(|_| EndpointError::ParseFloat)?;
        parse_weight(text).ok_or(EndpointError::ParseFloat)
    }
}
pub(super) fn format_weight(value: f64) -> String {
    if value == f64::INFINITY {
        "+Inf".into()
    } else if value == f64::NEG_INFINITY {
        "-Inf".into()
    } else {
        value.to_string()
    }
}

// strconv accepts Go literal underscores and hexadecimal binary exponents;
// Rust FromStr alone also accepts signed NaN, which Go rejects.
pub(super) fn parse_weight(text: &str) -> Option<f64> {
    let lower = text.to_ascii_lowercase();
    match lower.as_str() {
        "nan" => return Some(f64::NAN),
        "inf" | "+inf" | "infinity" | "+infinity" => return Some(f64::INFINITY),
        "-inf" | "-infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let unsigned = lower.strip_prefix(['+', '-']).unwrap_or(&lower);
    let hex = unsigned.starts_with("0x");
    let digit = |c: u8| {
        if hex {
            c.is_ascii_hexdigit()
        } else {
            c.is_ascii_digit()
        }
    };
    for (i, c) in lower.bytes().enumerate() {
        if c == b'_'
            && !(i > 0
                && i + 1 < lower.len()
                && digit(lower.as_bytes()[i + 1])
                && (digit(lower.as_bytes()[i - 1])
                    || (hex && i == lower.len() - unsigned.len() + 2)))
        {
            return None;
        }
    }
    let clean = lower.replace('_', "");
    if hex {
        return parse_hex(&clean);
    }
    if !clean.bytes().any(|c| c.is_ascii_digit()) {
        return None;
    }
    let value = clean.parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

// Build the exact binary significand, then round once to nearest, ties to even.
// This also handles subnormal values without intermediate floating point rounding.
fn parse_hex(text: &str) -> Option<f64> {
    let negative = text.starts_with('-');
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = unsigned.strip_prefix("0x")?.split_once('p')?;
    let exp_digits = exponent.strip_prefix(['+', '-']).unwrap_or(exponent);
    if exp_digits.is_empty() || !exp_digits.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let exp = exponent
        .parse::<i64>()
        .unwrap_or(if exponent.starts_with('-') {
            i64::MIN
        } else {
            i64::MAX
        });
    let mut bits = Vec::new();
    let mut after_point = false;
    let mut fraction = 0i64;
    for c in mantissa.chars() {
        if c == '.' && !after_point {
            after_point = true;
            continue;
        }
        let digit = c.to_digit(16)?;
        if after_point {
            fraction += 4;
        }
        bits.extend((0..4).rev().map(|b| (digit >> b) & 1));
    }
    if bits.is_empty() {
        return None;
    }
    let sign = u64::from(negative) << 63;
    let Some(first) = bits.iter().position(|b| *b != 0) else {
        return Some(f64::from_bits(sign));
    };
    let bits = &bits[first..];
    let top = exp
        .saturating_sub(fraction)
        .saturating_add(bits.len() as i64 - 1);
    if top > 1023 {
        return None;
    }
    if top < -1075 {
        return Some(f64::from_bits(sign));
    }
    let keep = (top + 1075).min(53) as usize;
    let mut significand = 0u64;
    for i in 0..keep {
        significand = (significand << 1) | u64::from(*bits.get(i).unwrap_or(&0));
    }
    if bits.get(keep) == Some(&1) && (significand & 1 != 0 || bits[keep + 1..].contains(&1)) {
        significand += 1;
    }
    let magnitude = if top < -1022 {
        significand
    } else {
        ((top + 1023) as u64 - 1) * (1 << 52) + significand
    };
    if magnitude >= 0x7ff0000000000000 {
        None
    } else {
        Some(f64::from_bits(sign | magnitude))
    }
}
