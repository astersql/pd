// Copyright 2026 AsterSQL.
//! Process-owned etcd SDK transports over the shared connection kernel.
use astersql_pd_mcs_server::{DelegateConnections, OwnedDelegateChannel};
use astersql_pd_storage::{EtcdMetaStore, StoreError};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

pub(crate) async fn connect(
    config: &crate::mcs_config::Config,
    cancel: &CancellationToken,
) -> Result<EtcdMetaStore, String> {
    let endpoints: Vec<String> = config
        .backend_endpoints
        .split(',')
        .map(String::from)
        .collect();
    let runtime = tokio::runtime::Handle::current();
    let plain = Arc::new(DelegateConnections::new(runtime.clone(), cancel, None));
    // Go returns nil TLS for CA-only file configuration. HTTPS then uses system
    // roots; an explicit HTTP endpoint drops even configured TLS credentials.
    let secure = Arc::new(DelegateConnections::new_with_rustls_factory(
        runtime,
        cancel,
        config.client_tls_policy(),
    ));
    EtcdMetaStore::connect_with_transport_factory(
        &endpoints,
        Duration::from_secs(2),
        move |targets| {
            let mut transports = Vec::with_capacity(targets.len());
            for target in targets {
                let pool = if target.starts_with("https://") {
                    secure.clone()
                } else if target.starts_with("http://") {
                    plain.clone()
                } else {
                    return Err(StoreError::InvalidArgument);
                };
                transports.push((target.clone(), pool));
            }
            OwnedDelegateChannel::new(transports).map_err(|_| StoreError::InvalidArgument)
        },
    )
    .await
    .map_err(|error| error.to_string())
}
