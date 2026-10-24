// Copyright 2026 AsterSQL.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Group {
    #[serde(deserialize_with = "null_default")]
    pub id: String,
    #[serde(deserialize_with = "null_default")]
    pub create_timestamp: u64,
    #[serde(skip_serializing_if = "is_zero")]
    #[serde(deserialize_with = "null_default")]
    pub leader_store_id: u64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    #[serde(deserialize_with = "null_default")]
    pub voter_store_ids: Vec<u64>,
}
fn is_zero(v: &u64) -> bool {
    *v == 0
}
impl std::fmt::Display for Group {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&serde_json::to_string(self).map_err(|_| std::fmt::Error)?)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GroupState {
    #[serde(flatten)]
    pub group: Group,
    #[serde(skip)]
    pub regular_scheduling_allowed: bool,
    #[serde(skip)]
    pub affinity_scheduling_allowed: bool,
    pub phase: String,
    pub range_count: usize,
    pub region_count: usize,
    pub affinity_region_count: usize,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Availability {
    #[default]
    Available,
    Degraded,
    Expired,
}
impl std::fmt::Display for Availability {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Available => "available",
            Self::Degraded => "degraded",
            Self::Expired => "expired",
        })
    }
}
#[derive(Clone, Debug, Default)]
pub struct RuntimeGroupInfo {
    pub group: Group,
    availability: Availability,
    degraded_expired_at: u64,
    pub range_count: usize,
    pub region_count: usize,
    pub affinity_region_count: usize,
}
impl RuntimeGroupInfo {
    pub fn availability(&self, now: u64) -> Availability {
        if self.availability == Availability::Degraded && now > self.degraded_expired_at {
            Availability::Expired
        } else {
            self.availability
        }
    }
    pub fn set_availability(&mut self, value: Availability, now: u64) {
        self.availability = self.availability(now);
        match value {
            Availability::Available | Availability::Expired => self.availability = value,
            Availability::Degraded if self.availability == Availability::Available => {
                self.availability = value;
                self.degraded_expired_at = now.wrapping_add(600);
            }
            _ => {}
        }
    }
    pub fn state(&self, now: u64) -> GroupState {
        let assigned = self.group.leader_store_id != 0 && !self.group.voter_store_ids.is_empty();
        let affinity = self.availability(now) == Availability::Available && assigned;
        GroupState {
            group: self.group.clone(),
            regular_scheduling_allowed: self.availability(now) == Availability::Expired
                || !assigned,
            affinity_scheduling_allowed: affinity,
            phase: if self.range_count == 0 || !affinity {
                "pending"
            } else if self.affinity_region_count > 0
                && self.region_count == self.affinity_region_count
            {
                "stable"
            } else {
                "preparing"
            }
            .into(),
            range_count: self.range_count,
            region_count: self.region_count,
            affinity_region_count: self.affinity_region_count,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error(
    "[PD:affinity:ErrInvalidGroupID]invalid group id {0}: should contain only letters (a-z, A-Z), numbers (0-9), hyphens (-) and underscores (_), and be 1-64 characters long"
)]
pub struct InvalidGroupId(pub String);
pub fn validate_group_id(id: &str) -> Result<(), InvalidGroupId> {
    if !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        Ok(())
    } else {
        Err(InvalidGroupId(id.into()))
    }
}

fn null_default<'de, D, T>(d: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    Ok(Option::<T>::deserialize(d)?.unwrap_or_default())
}
