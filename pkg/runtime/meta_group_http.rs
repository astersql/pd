// Copyright 2026 AsterSQL.
//! Host-owned fresh etcd health connections for the canonical directory manager.
use astersql_pd_keyspace::{GroupEndpointHealth, GroupUpdateError};
use std::{sync::Arc, time::Duration};
pub(crate) struct Health(
    pub Arc<crate::cluster_host::ClusterHost>,
    pub tokio_util::sync::CancellationToken,
    pub tokio_util::sync::CancellationToken,
);
#[async_trait::async_trait]
impl GroupEndpointHealth for Health {
    async fn check(&self, endpoint: &str) -> Result<(), String> {
        let config = self.0.meta_group_health_configuration().await?;
        tokio::select! {biased;_=self.1.cancelled()=>Err("context canceled".into()),_=self.2.cancelled()=>Err("context canceled".into()),r=check_endpoint(&config, endpoint)=>r}
    }
}
pub(crate) async fn check_endpoint(
    config: &astersql_pd_server_config::Config,
    endpoint: &str,
) -> Result<(), String> {
    let tls = config.security.tls.clone();
    let mut options = etcd_client::ConnectOptions::new()
        .with_require_leader(true)
        .with_connect_timeout(Duration::from_secs(10))
        .with_timeout(Duration::from_secs(10));
    if !tls.ca_path.is_empty()
        || tls.sslca_bytes.is_some()
        || !tls.cert_path.is_empty()
        || tls.ssl_cert_bytes.is_some()
        || endpoint.starts_with("https://")
    {
        let read = |path: &str, bytes: Option<Vec<u8>>| -> Result<Vec<u8>, String> {
            bytes.map(Ok).unwrap_or_else(|| {
                std::fs::read(path).map_err(|e| format!("failed to load TLS config: {e}"))
            })
        };
        let mut transport = etcd_client::TlsOptions::new();
        if !tls.ca_path.is_empty() || tls.sslca_bytes.is_some() {
            transport = transport.ca_certificate(etcd_client::Certificate::from_pem(read(
                &tls.ca_path,
                tls.sslca_bytes,
            )?));
        } else {
            transport = transport.with_native_roots();
        }
        if !tls.cert_path.is_empty() || tls.ssl_cert_bytes.is_some() {
            transport = transport.identity(etcd_client::Identity::from_pem(
                read(&tls.cert_path, tls.ssl_cert_bytes)?,
                read(&tls.key_path, tls.sslkey_bytes)?,
            ));
        }
        options = options.with_tls(transport);
    }
    let work = async {
        let mut client = etcd_client::Client::connect([endpoint], Some(options))
            .await
            .map_err(|_| "etcd health check failed".to_owned())?;
        match client.get("health", None).await {
            Ok(_) => Ok(()),
            Err(etcd_client::Error::GRpcStatus(status))
                if status.code() == tonic::Code::PermissionDenied =>
            {
                Ok(())
            }
            Err(_) => Err("etcd health check failed".into()),
        }
    };
    tokio::time::timeout(Duration::from_secs(10), work)
        .await
        .map_err(|_| "etcd health check failed".to_owned())?
}
pub(crate) fn update_error(error: GroupUpdateError) -> (u16, String) {
    match error {
        GroupUpdateError::Unhealthy {
            group,
            endpoint,
            message,
        } => (
            400,
            format!(
                "[PD:keyspace:ErrMetaServiceGroupUnhealthy]meta-service group etcd server is unhealthy: group {group} endpoint {endpoint}: {message}"
            ),
        ),
        GroupUpdateError::AssignedKeyspaces(id) => (
            400,
            format!(
                "[PD:keyspace:ErrGroupHasAssignedKeyspaces]cannot delete meta-service group with assigned keyspaces: {id}"
            ),
        ),
        other => {
            let message = other.to_string();
            if message.contains("transaction conflict") {
                (409,"[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back".into())
            } else {
                (500, message)
            }
        }
    }
}
