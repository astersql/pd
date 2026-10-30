// Copyright 2026 AsterSQL.
//! Explicit exclusive ownership of independent etcd members. Diagnostic bindings
//! never grant removal authority. Ownership and revocation survive PD restarts.
use astersql_pd_member::Leadership;
use astersql_pd_storage::{Compare, Consistency, MetaStore, Mutation, Read};
use etcd_client::{Client, ConnectOptions};
use std::{sync::Arc, time::Duration};

pub struct EtcdMemberAdmin {
    client: Client,
    cluster: u64,
    member: u64,
}
pub(crate) fn binding_key(cluster: u64, member: u64) -> Vec<u8> {
    format!("/pd/{cluster}/rust/member-ownership/{member}").into_bytes()
}
pub(crate) fn revoked_key(cluster: u64, member: u64) -> Vec<u8> {
    format!("/pd/{cluster}/rust/removed-members/{member}").into_bytes()
}
pub(crate) fn owner_key(cluster: u64, member: u64) -> Vec<u8> {
    format!("/pd/rust/etcd-member-owners/{cluster}/{member}").into_bytes()
}
async fn value(store: &dyn MetaStore, key: &[u8]) -> Result<Option<Vec<u8>>, String> {
    Ok(store
        .read(Read::range(key, b"", Consistency::Linearizable))
        .await
        .map_err(|e| e.to_string())?
        .entries
        .into_iter()
        .next()
        .map(|e| e.value))
}
impl EtcdMemberAdmin {
    /// The operator explicitly dedicates this real etcd member to one PD identity.
    /// Connections are bounded and the endpoint must identify exactly that member.
    pub async fn connect(endpoint: String, member: u64, timeout: Duration) -> Result<Self, String> {
        if member == 0 || timeout.is_zero() {
            return Err("nonzero dedicated member and timeout required".into());
        }
        let mut client = Client::connect(
            [endpoint],
            Some(
                ConnectOptions::new()
                    .with_connect_timeout(timeout)
                    .with_timeout(timeout),
            ),
        )
        .await
        .map_err(|e| e.to_string())?;
        let status = client.status().await.map_err(|e| e.to_string())?;
        let header = status.header().ok_or("missing etcd identity")?;
        if header.member_id() != member || header.cluster_id() == 0 {
            return Err("dedicated etcd member identity mismatch".into());
        }
        Ok(Self {
            cluster: header.cluster_id(),
            member,
            client,
        })
    }
    /// Atomically reserve both directions. Existing ownership is immutable, and
    /// revoked identities cannot rejoin, even with another backend member.
    pub(crate) async fn bind(
        &self,
        store: &dyn MetaStore,
        cluster: u64,
        member: u64,
    ) -> Result<(), String> {
        if store
            .consensus_cluster_id()
            .await
            .map_err(|e| e.to_string())?
            != Some(self.cluster)
        {
            return Err("member ownership and metadata use different etcd clusters".into());
        }
        let snapshot = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .map_err(|e| e.to_string())?;
        let guard = guard_compare(&snapshot);
        let diagnostic = format!("/pd/{cluster}/rust/diagnostic-members/{member}");
        let expected_diagnostic = format!("{}:{}", self.cluster, self.member);
        if snapshot.entries.iter().any(|e| {
            String::from_utf8_lossy(&e.key).contains("/rust/diagnostic-members/")
                && e.key != diagnostic.as_bytes()
                && e.value == expected_diagnostic.as_bytes()
        }) {
            return Err(
                "etcd member has shared diagnostic bindings; exclusive ownership refused".into(),
            );
        }
        let forward = binding_key(cluster, member);
        let reverse = owner_key(self.cluster, self.member);
        let binding = format!("{}:{}", self.cluster, self.member).into_bytes();
        let owner = format!("{cluster}:{member}").into_bytes();
        let revoked = revoked_key(cluster, member);
        let f = value(store, &forward).await?;
        let r = value(store, &reverse).await?;
        if f.as_ref().is_some_and(|v| v != &binding) || r.as_ref().is_some_and(|v| v != &owner) {
            return Err("dedicated etcd member ownership conflict".into());
        }
        let expected = |key: Vec<u8>, current: Option<Vec<u8>>| match current {
            Some(v) => Compare::Value(key, v),
            None => Compare::CreateRevision(key, 0),
        };
        let commit = store
            .transact(
                vec![
                    guard,
                    expected(forward.clone(), f),
                    expected(reverse.clone(), r),
                    Compare::CreateRevision(revoked, 0),
                ],
                vec![
                    Mutation::put(BINDING_GUARD, b"1"),
                    Mutation::put(&forward, &binding),
                    Mutation::put(&reverse, &owner),
                ],
            )
            .await
            .map_err(|e| e.to_string())?;
        if !commit.succeeded {
            return Err("member is revoked or ownership changed".into());
        }
        Ok(())
    }
    /// Removal is a recoverable two-phase operation, not a cross-system atomic
    /// transaction: revoke admission first, then remove consensus membership,
    /// then remove discovery/priority. Retry resumes from the durable intent.
    pub(crate) async fn remove(
        &self,
        store: Arc<dyn MetaStore>,
        term: &Leadership,
        cluster: u64,
        member: u64,
    ) -> Result<(), String> {
        let forward = binding_key(cluster, member);
        let binding = value(store.as_ref(), &forward)
            .await?
            .ok_or("target has no dedicated etcd ownership; shared backend removal refused")?;
        let text = std::str::from_utf8(&binding).map_err(|_| "invalid member ownership")?;
        let (backend, target) = text.split_once(':').ok_or("invalid member ownership")?;
        let backend: u64 = backend.parse().map_err(|_| "invalid member ownership")?;
        let target: u64 = target.parse().map_err(|_| "invalid member ownership")?;
        if backend != self.cluster || target == 0 {
            return Err("target belongs to another etcd cluster".into());
        }
        let reverse = owner_key(backend, target);
        let owner = format!("{cluster}:{member}").into_bytes();
        if value(store.as_ref(), &reverse).await?.as_ref() != Some(&owner) {
            return Err("dedicated member reverse ownership mismatch".into());
        }
        let mut client = self.client.clone();
        let list = client.member_list().await.map_err(|e| e.to_string())?;
        if list.header().map(|h| h.cluster_id()) != Some(backend) {
            return Err("etcd membership cluster mismatch".into());
        }
        let exists = list.members().iter().any(|m| m.id() == target);
        let revoked = revoked_key(cluster, member);
        let intent = value(store.as_ref(), &revoked).await?;
        if !exists && intent.is_none() {
            return Err("owned etcd member is absent without removal intent".into());
        }
        if exists && list.members().len() <= 1 {
            return Err("cannot remove the last dedicated etcd member".into());
        }
        let compares = vec![
            Compare::Value(forward.clone(), binding.clone()),
            Compare::Value(reverse, owner),
        ];
        let commit = term
            .transact(compares.clone(), vec![Mutation::put(&revoked, b"removing")])
            .await
            .map_err(|e| e.to_string())?;
        if !commit.succeeded {
            return Err("member ownership changed".into());
        }
        if exists {
            // An interrupted RPC may have committed. A retry reconciles MemberList.
            let response = client
                .member_remove(target)
                .await
                .map_err(|e| e.to_string())?;
            if response.header().map(|h| h.cluster_id()) != Some(backend) {
                return Err("etcd removal cluster mismatch".into());
            }
        }
        let commit = term
            .transact(
                compares,
                vec![
                    Mutation::Delete {
                        key: format!("/pd/{cluster}/rust/members/{member}").into_bytes(),
                    },
                    Mutation::Delete {
                        key: format!("/pd/{cluster}/member/{member}/leader_priority").into_bytes(),
                    },
                    Mutation::put(&revoked, b"removed"),
                ],
            )
            .await
            .map_err(|e| e.to_string())?;
        if !commit.succeeded {
            return Err("member removal cleanup requires retry".into());
        }
        Ok(())
    }
}

/// Fail closed on a durable revocation or unavailable membership metadata. The
/// caller owns polling and shutdown; no detached tasks or private runtime.
pub(crate) async fn ensure_admitted(
    store: &dyn MetaStore,
    cluster: u64,
    member: u64,
) -> Result<(), String> {
    if value(store, &revoked_key(cluster, member)).await?.is_some() {
        return Err("PD member has been revoked".into());
    }
    Ok(())
}

pub(crate) const BINDING_GUARD: &[u8] = b"/pd/rust/member-binding-guard";
pub(crate) fn guard_compare(snapshot: &astersql_pd_storage::Snapshot) -> Compare {
    Compare::ModRevision(
        BINDING_GUARD.to_vec(),
        snapshot
            .entries
            .iter()
            .find(|e| e.key == BINDING_GUARD)
            .map_or(0, |e| e.mod_revision),
    )
}
