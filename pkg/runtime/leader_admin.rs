// Copyright 2026 AsterSQL.
//! Consensus transfer for explicitly dedicated etcd deployments. PD IDs are
//! resolved through immutable ownership, never passed to etcd as backend IDs.
use crate::member_admin::{EtcdMemberAdmin, binding_key, owner_key, revoked_key};
use astersql_pd_member::Leadership;
use astersql_pd_storage::{Compare, Consistency, MetaStore, Read};
use std::sync::Arc;

const NO_TARGET: &str = "no valid pd to transfer etcd leader";
fn move_error(message: &str) -> String {
    // Go Wrap(...).GenWithStackByCause() repeats the underlying cause.
    format!("[PD:etcd:ErrEtcdMoveLeader]{message}: {message}")
}
impl EtcdMemberAdmin {
    pub(crate) async fn is_leader(&self) -> Result<bool, String> {
        let status = self
            .client
            .clone()
            .status()
            .await
            .map_err(|e| e.to_string())?;
        if status.header().map(|h| (h.cluster_id(), h.member_id()))
            != Some((self.cluster, self.member))
        {
            return Err("dedicated etcd member identity mismatch".into());
        }
        Ok(status.leader() == self.member)
    }
    pub(crate) async fn transfer(
        &self,
        store: Arc<dyn MetaStore>,
        term: &Leadership,
        cluster: u64,
        from: u64,
        target: Option<u64>,
    ) -> Result<(), String> {
        let mut client = self.client.clone();
        let members = client.member_list().await.map_err(|e| e.to_string())?;
        if members.header().map(|h| h.cluster_id()) != Some(self.cluster) {
            return Err("etcd membership cluster mismatch".into());
        }
        let snapshot = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        let value = |key: &[u8]| snapshot.entries.iter().find(|e| e.key == key);
        let valid = |pd: u64, backend: u64| {
            value(&binding_key(cluster, pd))
                .is_some_and(|e| e.value == format!("{}:{backend}", self.cluster).as_bytes())
                && value(&owner_key(self.cluster, backend))
                    .is_some_and(|e| e.value == format!("{cluster}:{pd}").as_bytes())
                && value(&revoked_key(cluster, pd)).is_none()
                && value(format!("/pd/{cluster}/rust/members/{pd}").as_bytes()).is_some()
        };
        if !valid(from, self.member) {
            return Err("dedicated member ownership mismatch".into());
        }
        let candidates: Vec<_> = members
            .members()
            .iter()
            .filter_map(|m| {
                let owner = value(&owner_key(self.cluster, m.id()))?;
                let owner = std::str::from_utf8(&owner.value).ok()?;
                let (namespace, pd) = owner.split_once(':')?;
                let pd: u64 = pd.parse().ok()?;
                if namespace != cluster.to_string() || !valid(pd, m.id()) {
                    return None;
                }
                if target.map_or(m.id() != self.member, |target| target == pd) {
                    Some((pd, m.id()))
                } else {
                    None
                }
            })
            .collect();
        let (pd, backend) = if target.is_none()
            && members.members().len() == 1
            && members.members()[0].id() == self.member
        {
            (from, self.member)
        } else {
            use rand::seq::SliceRandom;
            *candidates
                .choose(&mut rand::thread_rng())
                .ok_or(NO_TARGET)?
        };
        if pd != from {
            // Unlike embedded Go etcd, a dedicated backend can outlive its PD.
            // Reuse the real health transport before handing it consensus; the
            // registry revision is fenced below after this asynchronous probe.
            use prost::Message;
            let entry =
                value(format!("/pd/{cluster}/rust/members/{pd}").as_bytes()).ok_or(NO_TARGET)?;
            let member = astersql_pd_proto::pdpb::Member::decode(entry.value.as_slice())
                .map_err(|e| e.to_string())?;
            if member.member_id != pd {
                return Err(NO_TARGET.into());
            }
            let mut reachable = false;
            for url in &member.client_urls {
                if astersql_pd_server::http_health::ping_url(url).await {
                    reachable = true;
                    break;
                }
            }
            if !reachable {
                return Err(move_error(
                    "etcdserver: request timed out, leader transfer took too long",
                ));
            }
        }
        // Check registry, ownership and revocation in the same fenced transaction.
        // MoveLeader is a separate consensus RPC; uncertain/cancelled responses
        // are not rolled back. The role worker reconciles actual consensus state.
        let mut compares = Vec::new();
        for (pd, backend) in [(from, self.member), (pd, backend)] {
            for key in [
                binding_key(cluster, pd),
                owner_key(self.cluster, backend),
                format!("/pd/{cluster}/rust/members/{pd}").into_bytes(),
            ] {
                let entry = value(&key).ok_or(NO_TARGET)?;
                compares.push(Compare::ModRevision(key, entry.mod_revision));
            }
            compares.push(Compare::CreateRevision(revoked_key(cluster, pd), 0));
        }
        if !term
            .transact(compares, vec![])
            .await
            .map_err(|e| e.to_string())?
            .succeeded
        {
            return Err(NO_TARGET.into());
        }
        if !self.is_leader().await? {
            return Err(move_error("etcdserver: not leader"));
        }
        if backend == self.member {
            return Ok(());
        }
        client.move_leader(backend).await.map_err(|e| {
            let message = match e {
                etcd_client::Error::GRpcStatus(status) => {
                    // gRPC code 4 is DeadlineExceeded.
                    if status.code() as i32 == 4 || status.message() == "Timeout expired" {
                        "etcdserver: request timed out, leader transfer took too long".to_owned()
                    } else {
                        status.message().to_owned()
                    }
                }
                error => error.to_string(),
            };
            move_error(&message)
        })?;
        Ok(())
    }
}
