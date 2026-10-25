// Copyright 2026 AsterSQL.
//! Witness conversions acknowledged by heartbeat state, independent of peer role.
use crate::{
    OpInfluence, PeerStepError, TransferLeader,
    step_add::{slow_timeout, validate},
};
use astersql_pd_core::{BasicCluster, RegionInfo};
use astersql_pd_schedule_config::SharedConfigProvider;
use astersql_pd_schedule_hbstream::{BatchSwitchWitness as BatchCommand, Operation, SwitchWitness};
use astersql_pd_storelimit::Type;
use std::{fmt, time::Duration};

/// Convert a matching peer into a witness and shed its data.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BecomeWitness {
    pub peer_id: u64,
    pub store_id: u64,
}
/// Convert a witness into a data peer, waiting for its snapshot to finish.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BecomeNonWitness {
    pub peer_id: u64,
    pub store_id: u64,
    pub send_store: u64,
}
/// Atomically acknowledge all conversions, preserving command order and duplicates.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BatchSwitchWitness {
    pub to_witnesses: Vec<BecomeWitness>,
    pub to_non_witnesses: Vec<BecomeNonWitness>,
}
fn command(peer_id: u64, is_witness: bool) -> Operation {
    Operation {
        switch_witnesses: Some(BatchCommand {
            switch_witnesses: vec![Some(SwitchWitness {
                peer_id,
                is_witness,
            })],
        }),
        ..Default::default()
    }
}
fn check<C: SharedConfigProvider>(
    c: &BasicCluster,
    cfg: &C,
    r: &RegionInfo,
    store: u64,
    id: u64,
    health: bool,
    now: i128,
) -> Result<(), PeerStepError> {
    validate(c, cfg, store, health, now)?;
    if r.get_store_peer(store).is_some_and(|p| p.id == id) {
        Ok(())
    } else {
        Err(PeerStepError::MissingPeer)
    }
}
impl BecomeWitness {
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        u64::from(self.is_finish(r))
    }
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        r.get_store_peer(self.store_id)
            .is_some_and(|p| p.id == self.peer_id && p.is_witness)
    }
    /// Witness conversion ignores the optional unhealthy check, but still rejects down stores.
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        c: &BasicCluster,
        cfg: &C,
        r: &RegionInfo,
        _health: bool,
        now: i128,
    ) -> Result<(), PeerStepError> {
        check(c, cfg, r, self.store_id, self.peer_id, false, now)
    }
    pub fn influence(&self, inf: &OpInfluence, r: &RegionInfo) {
        inf.update_store(self.store_id, |s| {
            s.witness_count = s.witness_count.wrapping_add(1);
            s.region_size = s.region_size.wrapping_sub(r.stats().approximate_size);
            s.adjust_step_cost(
                Type::REMOVE_PEER,
                r.stats().approximate_size,
                r.stats().approximate_keys,
            );
        });
    }
    pub fn timeout(&self, size: i64) -> Duration {
        TransferLeader::default().timeout(size)
    }
    pub fn get_cmd(&self, _r: &RegionInfo, _v2: bool) -> Operation {
        command(self.peer_id, true)
    }
}
impl BecomeNonWitness {
    /// Confver changes before snapshot completion; nil-safe ID-zero behavior follows Go.
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        u64::from(
            r.get_store_peer(self.store_id).map_or(0, |p| p.id) == self.peer_id
                && !r
                    .get_store_peer(self.store_id)
                    .is_some_and(|p| p.is_witness),
        )
    }
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        r.get_store_peer(self.store_id).is_some_and(|p| {
            p.id == self.peer_id && !p.is_witness && r.get_pending_peer(p.id).is_none()
        })
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        c: &BasicCluster,
        cfg: &C,
        r: &RegionInfo,
        health: bool,
        now: i128,
    ) -> Result<(), PeerStepError> {
        check(c, cfg, r, self.store_id, self.peer_id, health, now)
    }
    pub fn influence(&self, inf: &OpInfluence, r: &RegionInfo) {
        let size = r.stats().approximate_size;
        inf.update_store(self.store_id, |s| {
            s.witness_count = s.witness_count.wrapping_sub(1);
            s.region_size = s.region_size.wrapping_add(size);
            s.adjust_step_cost(Type::ADD_PEER, size, r.stats().approximate_keys);
        });
        if self.send_store != 0 {
            inf.update_store(self.send_store, |s| {
                s.add_step_cost(Type::SEND_SNAPSHOT, size)
            });
        }
    }
    pub fn timeout(&self, size: i64) -> Duration {
        slow_timeout(size)
    }
    pub fn get_cmd(&self, _r: &RegionInfo, _v2: bool) -> Operation {
        command(self.peer_id, false)
    }
}
impl BatchSwitchWitness {
    pub fn conf_ver_changed(&self, r: &RegionInfo) -> u64 {
        if self.to_witnesses.iter().all(|w| w.conf_ver_changed(r) != 0)
            && self
                .to_non_witnesses
                .iter()
                .all(|n| n.conf_ver_changed(r) != 0)
        {
            (self.to_witnesses.len() as u64).wrapping_add(self.to_non_witnesses.len() as u64)
        } else {
            0
        }
    }
    pub fn is_finish(&self, r: &RegionInfo) -> bool {
        self.to_witnesses.iter().all(|w| w.is_finish(r))
            && self.to_non_witnesses.iter().all(|n| n.is_finish(r))
    }
    pub fn check_in_progress<C: SharedConfigProvider>(
        &self,
        c: &BasicCluster,
        cfg: &C,
        r: &RegionInfo,
        health: bool,
        now: i128,
    ) -> Result<(), PeerStepError> {
        for w in &self.to_witnesses {
            w.check_in_progress(c, cfg, r, health, now)?;
        }
        for n in &self.to_non_witnesses {
            n.check_in_progress(c, cfg, r, health, now)?;
        }
        Ok(())
    }
    pub fn influence(&self, inf: &OpInfluence, r: &RegionInfo) {
        for w in &self.to_witnesses {
            w.influence(inf, r);
        }
        for n in &self.to_non_witnesses {
            n.influence(inf, r);
        }
    }
    /// Signed nanoseconds preserve Go time.Duration multiplication, including overflow.
    pub fn timeout_nanos(&self, size: i64) -> i64 {
        (slow_timeout(size).as_nanos() as i64)
            .wrapping_mul((self.to_non_witnesses.len() as i64).wrapping_add(1))
    }
    /// Duration view retains the signed source duration's two's-complement bit pattern.
    /// Consumers needing signed deadlines should use timeout_nanos.
    pub fn timeout(&self, size: i64) -> Duration {
        Duration::from_nanos(self.timeout_nanos(size) as u64)
    }
    pub fn get_cmd(&self, _r: &RegionInfo, _v2: bool) -> Operation {
        let switches = self
            .to_witnesses
            .iter()
            .map(|w| {
                Some(SwitchWitness {
                    peer_id: w.peer_id,
                    is_witness: true,
                })
            })
            .chain(self.to_non_witnesses.iter().map(|n| {
                Some(SwitchWitness {
                    peer_id: n.peer_id,
                    is_witness: false,
                })
            }))
            .collect();
        Operation {
            switch_witnesses: Some(BatchCommand {
                switch_witnesses: switches,
            }),
            ..Default::default()
        }
    }
}
impl fmt::Display for BecomeWitness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "switch peer {} on store {} to witness",
            self.peer_id, self.store_id
        )
    }
}
impl fmt::Display for BecomeNonWitness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "switch peer {} on store {} to non-witness",
            self.peer_id, self.store_id
        )
    }
}
impl fmt::Display for BatchSwitchWitness {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("batch switch witness")?;
        for w in &self.to_witnesses {
            write!(f, ", {w}")?;
        }
        for n in &self.to_non_witnesses {
            write!(f, ", {n}")?;
        }
        Ok(())
    }
}
