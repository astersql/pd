// Copyright 2026 AsterSQL.
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_storage::{Consistency, MetaStore, Read};
use prost::Message;
use std::{process::ExitCode, sync::Arc};
pub(crate) fn main(args: Vec<String>) -> ExitCode {
    let config = match crate::mcs_config::Config::parse(args) {
        Ok(config) => config,
        Err(_) => {
            eprintln!("microservice configuration could not be loaded or validated");
            return ExitCode::FAILURE;
        }
    };
    if super::cli_logging::init(&config.log).is_err() {
        eprintln!("microservice logger initialization failed");
        return ExitCode::FAILURE;
    }
    if config.validate_client_credentials().is_err() {
        eprintln!("microservice TLS credentials could not be loaded or validated");
        return ExitCode::FAILURE;
    }
    let listen = config.listen_addr.clone();
    let address = config.advertise_listen_addr.clone();
    let name = config.name.clone();
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())
        .and_then(|runtime| {
            runtime.block_on(async {
                let cancel = tokio_util::sync::CancellationToken::new();
                let _transport_lifetime = cancel.clone().drop_guard();
                let backend_store = crate::backend_transport::connect(&config, &cancel).await?;
                let store: Arc<dyn MetaStore> = Arc::new(backend_store.clone());
                let row = store
                    .read(Read::range(
                        b"/pd/cluster_id",
                        b"",
                        Consistency::Linearizable,
                    ))
                    .await
                    .map_err(|e| e.to_string())?
                    .entries
                    .into_iter()
                    .next()
                    .ok_or("PD cluster identity missing")?;
                let cluster = u64::from_be_bytes(
                    row.value
                        .as_slice()
                        .try_into()
                        .map_err(|_| "invalid cluster identity")?,
                );
                let endpoint = astersql_pd_runtime::net_address::Endpoint::parse(&listen)
                    .map_err(|e| e.to_string())?;
                let listener = endpoint.bind().await.map_err(|e| e.to_string())?;
                let advertised = astersql_pd_runtime::net_address::Endpoint::parse(&address)
                    .map_err(|e| e.to_string())?
                    .advertised_url(listener.local_addr().map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                let listener = config.listener(listener)?;
                let log_file = config.log.file.filename.clone();
                let participant_name = name.clone();
                let participant_address = advertised.clone();
                let config = astersql_pd_tso::GroupWatchConfig {
                    store: store.clone(),
                    runtime: tokio::runtime::Handle::current(),
                    cancel: cancel.clone(),
                    cluster_id: cluster,
                    address: advertised.clone(),
                    bootstrap_keyspace: 0,
                    participant: Arc::new(move |group| {
                        astersql_pd_proto::tsopb::Participant {
                            name: format!("{participant_name}-{group:05}"),
                            listen_urls: vec![participant_address.clone()],
                            ..Default::default()
                        }
                        .encode_to_vec()
                    }),
                    clock: Arc::new(astersql_pd_tso::SystemClock),
                    lease_seconds: 3,
                    save_interval_millis: 1000,
                };
                let leader_key = format!("/pd/{cluster}/leader");
                let pd_url = store
                    .read(Read::range(
                        leader_key.as_bytes(),
                        b"",
                        Consistency::Linearizable,
                    ))
                    .await
                    .map_err(|e| e.to_string())?
                    .entries
                    .into_iter()
                    .next()
                    .and_then(|e| astersql_pd_proto::pdpb::Member::decode(e.value.as_slice()).ok())
                    .and_then(|p| p.client_urls.into_iter().next())
                    .ok_or("PD leader unavailable")?;
                let groups = Arc::new(astersql_pd_keyspace::KeyspaceGroupManager::new(
                    store,
                    ClusterPaths::new(cluster),
                ));
                let entry = astersql_pd_mcs_discovery::ServiceRegistryEntry {
                    name,
                    service_addr: advertised,
                    version: env!("CARGO_PKG_VERSION").into(),
                    git_hash: option_env!("PD_GIT_HASH").unwrap_or("unknown").into(),
                    deploy_path: std::env::current_exe()
                        .ok()
                        .and_then(|p| p.parent().map(|p| p.display().to_string()))
                        .unwrap_or_default(),
                    start_timestamp: std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_err(|e| e.to_string())?
                        .as_secs() as i64,
                    ..Default::default()
                };
                let host = astersql_pd_mcs_tso_server::TsoServiceHost::start_with_log_file(
                    config,
                    groups,
                    listener,
                    Some(pd_url),
                    entry,
                    log_file,
                )
                .await
                .map_err(|e| e.to_string())?;
                super::entry::shutdown_signal()
                    .await
                    .map_err(|e| e.to_string())?;
                host.shutdown().await.map_err(|e| e.to_string())
            })
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tso startup or runtime failed: {e}");
            ExitCode::FAILURE
        }
    }
}
