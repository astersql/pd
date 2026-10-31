// Copyright 2026 AsterSQL.
//! Fresh admission checks for the external MetaStore topology.
use crate::{NodeConfig, NodeError, net_address::Endpoint};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use prost::Message;
use tokio_util::sync::CancellationToken;

fn canonical(raw: &str) -> Result<String, NodeError> {
    Ok(Endpoint::parse(raw.trim())?.url())
}

/// Checks fresh admission before member registration or any worker is started.
/// `join` identifies an already published PD member in the supplied MetaStore;
/// it never adds an etcd Raft member or interprets an embedded-etcd data directory.
/// An empty join preserves normal bootstrap/restart semantics. Like the Go
/// precheck this is a snapshot check, not an atomic URL reservation.
pub async fn check(
    store: &dyn MetaStore,
    config: &NodeConfig,
    join: &str,
    cancel: &CancellationToken,
) -> Result<(), NodeError> {
    check_advertised(store, config, join, &config.client_url, cancel).await
}

// The host resolves :0 before this check while preserving raw self-join and
// unfinished-peer matching semantics from the original configuration.
pub(crate) async fn check_advertised(
    store: &dyn MetaStore,
    config: &NodeConfig,
    join: &str,
    advertised_client_url: &str,
    cancel: &CancellationToken,
) -> Result<(), NodeError> {
    if join.is_empty() {
        return Ok(());
    }
    if join == config.client_url {
        return Err("join self is forbidden".into());
    }
    let targets = join
        .split(',')
        .map(canonical)
        .collect::<Result<Vec<_>, _>>()?;
    let clients = advertised_client_url
        .split(',')
        .map(canonical)
        .collect::<Result<Vec<_>, _>>()?;
    let mut peers = config
        .peer_url
        .split(',')
        .map(canonical)
        .collect::<Result<Vec<_>, _>>()?;
    peers.sort();
    let key = format!("/pd/{}/rust/members/", config.cluster_id);
    let snapshot = tokio::select! { biased;
        _ = cancel.cancelled() => return Err("node is stopped".into()),
        result = store.read(Read::prefix(key.as_bytes(), Consistency::Linearizable)) => result?,
    };
    let members = snapshot
        .entries
        .iter()
        .map(|entry| astersql_pd_proto::pdpb::Member::decode(entry.value.as_slice()))
        .collect::<Result<Vec<_>, _>>()?;
    // Preserve Go's abnormal-member check before the duplicate-name check.
    for member in &members {
        if member.name.is_empty() {
            // Go recovery matching uses the original peer URL multiset.
            let mut raw = member
                .peer_urls
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>();
            let mut own = config.peer_url.split(',').collect::<Vec<_>>();
            raw.sort();
            own.sort();
            if raw != own {
                return Err(format!(
                    "there is a member {} that has not joined successfully",
                    member.member_id
                )
                .into());
            }
        }
    }
    if members.iter().any(|member| member.name == config.name) {
        return Err("missing data or join a duplicated pd".into());
    }
    if members
        .iter()
        .any(|member| member.member_id == config.member_id)
    {
        return Err("member identity conflict".into());
    }
    let mut target_found = false;
    for member in &members {
        let mut owned_peers = member
            .peer_urls
            .iter()
            .map(|url| canonical(url))
            .collect::<Result<Vec<_>, _>>()?;
        owned_peers.sort();
        for owned in &member.client_urls {
            // Empty client lists are represented by an empty vector, never an endpoint.
            let owned = canonical(owned)?;
            target_found |= targets.contains(&owned);
            if owned_peers != peers && clients.contains(&owned) {
                return Err(format!(
                    "advertise-client-urls {:?} is already used by member {:?} (id {})",
                    advertised_client_url, member.name, member.member_id
                )
                .into());
            }
        }
    }
    if !target_found {
        return Err("join target is not registered in external MetaStore".into());
    }
    Ok(())
}
