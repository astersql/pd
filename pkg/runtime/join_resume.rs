// Copyright 2026 AsterSQL.
//! Durable admission intent for PD identities in the external MetaStore.
use crate::{NodeConfig, NodeError};
use astersql_pd_storage::{Compare, Consistency, MetaStore, Mutation, Read};
use prost::Message;

fn key(config: &NodeConfig) -> String {
    format!(
        "/pd/{}/rust/join-pending/{}",
        config.cluster_id, config.member_id
    )
}
pub(crate) fn value(config: &NodeConfig) -> Vec<u8> {
    astersql_pd_proto::pdpb::Member {
        name: config.name.clone(),
        member_id: config.member_id,
        client_urls: vec![config.client_url.clone()],
        peer_urls: vec![config.peer_url.clone()],
        ..Default::default()
    }
    .encode_to_vec()
}
pub(crate) async fn pending(store: &dyn MetaStore, config: &NodeConfig) -> Result<bool, NodeError> {
    let key = key(config);
    Ok(store
        .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
        .await?
        .entries
        .iter()
        .any(|entry| entry.key == key.as_bytes() && entry.value == value(config)))
}
/// Atomically records an unfinished admission with its immutable member identity.
/// Never overwrites an existing identity, tombstone, or another admission intent.
pub(crate) async fn register(store: &dyn MetaStore, config: &NodeConfig) -> Result<(), NodeError> {
    let member = format!(
        "/pd/{}/rust/members/{}",
        config.cluster_id, config.member_id
    );
    let pending_key = key(config);
    let value = value(config);
    let result = store
        .transact(
            vec![
                Compare::CreateRevision(member.as_bytes().to_vec(), 0),
                Compare::CreateRevision(pending_key.as_bytes().to_vec(), 0),
                Compare::CreateRevision(
                    format!(
                        "/pd/{}/rust/removed-members/{}",
                        config.cluster_id, config.member_id
                    )
                    .into_bytes(),
                    0,
                ),
            ],
            vec![
                Mutation::put(member.as_bytes(), &value),
                Mutation::put(pending_key.as_bytes(), &value),
            ],
        )
        .await?;
    if !result.succeeded && !pending(store, config).await? {
        return Err("member identity conflict".into());
    }
    Ok(())
}
pub(crate) async fn complete(store: &dyn MetaStore, config: &NodeConfig) -> Result<(), NodeError> {
    let key = key(config);
    let result = store
        .transact(
            vec![Compare::Value(key.as_bytes().to_vec(), value(config))],
            vec![Mutation::Delete {
                key: key.into_bytes(),
            }],
        )
        .await?;
    if !result.succeeded {
        return Err("join admission intent changed".into());
    }
    Ok(())
}
