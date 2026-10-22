// Copyright 2026 AsterSQL.
//! Stable Go-compatible region diagnostics without protocol dependencies.
use crate::{Peer, PeerRole, RegionMeta};
use std::fmt::{self, Write};

/// Balance range scheduling rule. Unrecognized spellings map to Unknown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Rule {
    LeaderScatter,
    PeerScatter,
    LearnerScatter,
    #[default]
    Unknown,
}
impl Rule {
    /// Parse the exact scheduler spelling.
    pub fn new(value: &str) -> Self {
        match value {
            "leader-scatter" => Self::LeaderScatter,
            "peer-scatter" => Self::PeerScatter,
            "learner-scatter" => Self::LearnerScatter,
            _ => Self::Unknown,
        }
    }
    /// Encode the canonical JSON string without requiring a serializer.
    pub fn marshal_json(self) -> Vec<u8> {
        format!("\"{self}\"").into_bytes()
    }
    /// Mirrors the Go hook's exact raw-byte matching, including its fallback.
    pub fn unmarshal_json(raw: &[u8]) -> Self {
        match raw {
            b"\"leader-scatter\"" => Self::LeaderScatter,
            b"\"peer-scatter\"" => Self::PeerScatter,
            b"\"learner-scatter\"" => Self::LearnerScatter,
            _ => Self::Unknown,
        }
    }
}
impl fmt::Display for Rule {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LeaderScatter => "leader-scatter",
            Self::PeerScatter => "peer-scatter",
            Self::LearnerScatter => "learner-scatter",
            Self::Unknown => "unknown",
        })
    }
}
/// Encode each byte as two uppercase hexadecimal ASCII bytes.
pub fn hex_region_key(key: &[u8]) -> Vec<u8> {
    hex_region_key_str(key).into_bytes()
}
/// Encode a key as uppercase hexadecimal text.
pub fn hex_region_key_str(key: &[u8]) -> String {
    let mut out = String::with_capacity(key.len() * 2);
    for byte in key {
        write!(out, "{byte:02X}").expect("string write");
    }
    out
}
/// Describe both key bounds, including unchanged bounds.
pub fn diff_region_key_info(origin: &RegionMeta, other: &RegionMeta) -> String {
    let key = |name: &str, a: &[u8], b: &[u8]| {
        if a == b {
            format!("{name}:{{{}}}", hex_region_key_str(a))
        } else {
            format!(
                "{name} Changed:{{{}}} -> {{{}}}",
                hex_region_key_str(a),
                hex_region_key_str(b)
            )
        }
    };
    format!(
        "{}, {}",
        key(
            "StartKey",
            &origin.key_range.start_key,
            &other.key_range.start_key
        ),
        key(
            "EndKey",
            &origin.key_range.end_key,
            &other.key_range.end_key
        )
    )
}
fn peer_text(peer: &Peer) -> String {
    let mut out = String::new();
    scalar(&mut out, "id", peer.id);
    scalar(&mut out, "store_id", peer.store_id);
    let role = match peer.role {
        PeerRole::Voter => None,
        PeerRole::Learner => Some("Learner".into()),
        PeerRole::IncomingVoter => Some("IncomingVoter".into()),
        PeerRole::DemotingVoter => Some("DemotingVoter".into()),
        PeerRole::Unknown(0) => None,
        PeerRole::Unknown(n) => Some(n.to_string()),
    };
    if let Some(role) = role {
        write!(out, "role:{role} ").expect("string write");
    }
    if peer.is_witness {
        out.push_str("is_witness:true ");
    }
    out
}
fn scalar(out: &mut String, name: &str, n: u64) {
    if n != 0 {
        write!(out, "{name}:{n} ").expect("string write");
    }
}
/// List removals before additions, preserving input order and full peer equality.
pub fn diff_region_peers_info(origin: &RegionMeta, other: &RegionMeta) -> String {
    let mut out = Vec::new();
    for (label, from, to) in [
        ("Remove", &origin.peers, &other.peers),
        ("Add", &other.peers, &origin.peers),
    ] {
        for peer in from {
            if !to.contains(peer) {
                out.push(format!("{label} peer:{{{}}}", peer_text(peer)));
            }
        }
    }
    out.join(",")
}
/// Borrowed metadata view; formatting does not mutate keys.
pub struct HexRegionMeta<'a>(Option<&'a RegionMeta>);
/// Wrap metadata for diagnostics; formatting absent metadata panics as in Go.
pub fn region_to_hex_meta(meta: Option<&RegionMeta>) -> HexRegionMeta<'_> {
    HexRegionMeta(meta)
}
fn meta_text(meta: &RegionMeta) -> String {
    let mut out = String::new();
    scalar(&mut out, "id", meta.id);
    for (name, key) in [
        ("start_key", &meta.key_range.start_key),
        ("end_key", &meta.key_range.end_key),
    ] {
        if !key.is_empty() {
            write!(out, "{name}:\"{}\" ", hex_region_key_str(key)).expect("string write");
        }
    }
    if let Some(epoch) = meta.epoch {
        out.push_str("region_epoch:<");
        scalar(&mut out, "conf_ver", epoch.conf_ver);
        scalar(&mut out, "version", epoch.version);
        out.push_str("> ");
    }
    for peer in &meta.peers {
        write!(out, "peers:<{}> ", peer_text(peer)).expect("string write");
    }
    if meta.is_in_flashback {
        out.push_str("is_in_flashback:true ");
    }
    scalar(&mut out, "flashback_start_ts", meta.flashback_start_ts);
    out
}
impl fmt::Display for HexRegionMeta<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Go HexRegionMeta.String dereferences the nil clone for absent metadata.
        let text = meta_text(self.0.expect("cannot format absent region metadata"));
        f.write_str(text.trim())
    }
}
/// Borrowed region collection formatted with compact protobuf-compatible spacing.
pub struct HexRegionsMeta<'a>(&'a [RegionMeta]);
/// Wrap a region slice without copying or mutating its metadata.
pub fn regions_to_hex_meta(regions: &[RegionMeta]) -> HexRegionsMeta<'_> {
    HexRegionsMeta(regions)
}
impl fmt::Display for HexRegionsMeta<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text: String = self.0.iter().map(meta_text).collect();
        f.write_str(text.trim())
    }
}
