// Copyright 2026 AsterSQL.
//! Transitional diagnostics for an explicitly bound independent etcd member.
//! This does not embed a Go runtime or assert process/topology equivalence.
use astersql_pd_server::http_health::LocalConsensusProbe;
use astersql_pd_storage::{Compare, Consistency, MetaStore, Mutation, Read};
use etcd_client::{AlarmAction, AlarmType, Client, ConnectOptions, GetOptions};
use std::{future::Future, time::Duration};

/// A single endpoint pinned to an operator-selected consensus member and cluster.
/// Every check verifies identity again, so DNS/proxy changes fail closed.
pub struct EtcdMemberDiagnostics {
    client: Client,
    member: u64,
    cluster: u64,
    timeout: Duration,
}
impl EtcdMemberDiagnostics {
    pub async fn connect(endpoint: String, member: u64, timeout: Duration) -> Result<Self, String> {
        if member == 0 || timeout.is_zero() {
            return Err("nonzero etcd member identity and timeout are required".into());
        }
        let options = ConnectOptions::new().with_connect_timeout(timeout);
        let mut client = tokio::time::timeout(timeout, Client::connect([endpoint], Some(options)))
            .await
            .map_err(|_| "etcd diagnostic connection timed out")?
            .map_err(|e| e.to_string())?;
        let status = tokio::time::timeout(timeout, client.status())
            .await
            .map_err(|_| "etcd diagnostic status timed out")?
            .map_err(|e| e.to_string())?;
        let header = status.header().ok_or("etcd status has no identity")?;
        if header.member_id() != member || header.cluster_id() == 0 {
            return Err("etcd diagnostic member identity mismatch".into());
        }
        Ok(Self {
            client,
            member,
            cluster: header.cluster_id(),
            timeout,
        })
    }
    async fn bounded<T>(
        &self,
        operation: impl Future<Output = Result<T, etcd_client::Error>>,
    ) -> Result<T, String> {
        tokio::time::timeout(self.timeout, operation)
            .await
            .map_err(|_| "etcd diagnostic request timed out")?
            .map_err(|e| e.to_string())
    }
    fn verify_header(&self, header: Option<&etcd_client::ResponseHeader>) -> Result<(), String> {
        let header = header.ok_or("etcd response has no identity")?;
        if header.member_id() != self.member || header.cluster_id() != self.cluster {
            return Err("etcd diagnostic member identity mismatch".into());
        }
        Ok(())
    }
    /// Verify that diagnostics and canonical metadata refer to the same backend cluster.
    pub async fn validate_store(&self, store: &dyn MetaStore) -> Result<(), String> {
        if store
            .consensus_cluster_id()
            .await
            .map_err(|e| e.to_string())?
            != Some(self.cluster)
        {
            return Err("diagnostics and metadata belong to different etcd clusters".into());
        }
        Ok(())
    }
    /// Validate the metadata backend and atomically persist the PD/member binding.
    /// Rebinding a persisted PD identity requires an explicit operator migration.
    pub async fn bind(
        &self,
        store: &dyn MetaStore,
        pd_cluster: u64,
        pd_member: u64,
    ) -> Result<(), String> {
        self.validate_store(store).await?;
        let key = format!("/pd/{pd_cluster}/rust/diagnostic-members/{pd_member}").into_bytes();
        let value = format!("{}:{}", self.cluster, self.member).into_bytes();
        let snapshot = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        let owner_key = crate::member_admin::owner_key(self.cluster, self.member);
        let owner = format!("{pd_cluster}:{pd_member}");
        if snapshot
            .entries
            .iter()
            .find(|e| e.key == owner_key)
            .is_some_and(|e| e.value != owner.as_bytes())
        {
            return Err("etcd member is exclusively owned by another PD member".into());
        }
        let existing = snapshot.entries.iter().find(|e| e.key == key);
        if existing.is_some_and(|e| e.value != value) {
            return Err("persisted PD to etcd member binding mismatch".into());
        }
        let commit = store
            .transact(
                vec![
                    crate::member_admin::guard_compare(&snapshot),
                    Compare::ModRevision(key.clone(), existing.map_or(0, |e| e.mod_revision)),
                ],
                vec![
                    Mutation::put(&key, &value),
                    Mutation::put(crate::member_admin::BINDING_GUARD, b"1"),
                ],
            )
            .await
            .map_err(|e| e.to_string())?;
        if !commit.succeeded {
            return Err("member binding changed concurrently; retry startup".into());
        }
        let snapshot = store
            .read(Read::range(&key, b"", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        if !snapshot
            .entries
            .iter()
            .any(|entry| entry.key == key && entry.value == value)
        {
            return Err("persisted PD to etcd member binding mismatch".into());
        }
        Ok(())
    }
    pub async fn check(&self, probe: LocalConsensusProbe) -> Result<(), String> {
        let mut client = self.client.clone();
        let status = self.bounded(client.status()).await?;
        self.verify_header(status.header())?;
        match probe {
            LocalConsensusProbe::NonLearner => {
                if status.is_learner() {
                    Err("member is a learner".into())
                } else {
                    Ok(())
                }
            }
            LocalConsensusProbe::DataCorruption => {
                let alarms = self
                    .bounded(client.alarm(AlarmAction::Get, AlarmType::Corrupt, None))
                    .await?;
                self.verify_header(alarms.header())?;
                if alarms
                    .alarms()
                    .iter()
                    .any(|alarm| alarm.alarm() == AlarmType::Corrupt)
                {
                    Err("etcd data corruption alarm is active".into())
                } else {
                    Ok(())
                }
            }
            LocalConsensusProbe::SerializableRead | LocalConsensusProbe::LinearizableRead => {
                let mut options = GetOptions::new()
                    .with_all_keys()
                    .with_keys_only()
                    .with_limit(1);
                let serializable = matches!(probe, LocalConsensusProbe::SerializableRead);
                if serializable {
                    options = options.with_serializable();
                }
                self.bounded(client.get(Vec::<u8>::new(), Some(options)))
                    .await
                    .and_then(|response| self.verify_header(response.header()))
                    .map_err(|error| {
                        format!(
                            "etcd {} read failed: {error}",
                            if serializable {
                                "serializable"
                            } else {
                                "linearizable"
                            }
                        )
                    })
            }
        }
    }
}
