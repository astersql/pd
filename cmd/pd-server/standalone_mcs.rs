// Copyright 2026 AsterSQL.
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{process::ExitCode, sync::Arc};
pub(crate) fn main(service: &str, args: Vec<String>) -> ExitCode {
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
                let controller = config.controller.clone();
                let host = match service {
                    "scheduling" => {
                        let delegates =
                            config.delegates(tokio::runtime::Handle::current(), &cancel)?;
                        Host::Scheduling(
                            astersql_pd_runtime::SchedulingHost::start_with_log_file(
                                astersql_pd_runtime::SchedulingHostConfig {
                                    runtime: tokio::runtime::Handle::current(),
                                    cancel,
                                    store,
                                    cluster_id: cluster,
                                    entry,
                                    delegates,
                                    registry: Arc::new(prometheus::Registry::new()),
                                    pd_leader_lookup: Arc::new(move || {
                                        let backend = backend_store.clone();
                                        Box::pin(async move {
                                            backend
                                                .consensus_leader_client_url()
                                                .await
                                                .map_err(|e| e.to_string())
                                        })
                                    }),
                                },
                                listener,
                                config.log.file.filename.clone(),
                            )
                            .await?,
                        )
                    }
                    "router" => {
                        let delegates =
                            config.delegates(tokio::runtime::Handle::current(), &cancel)?;
                        Host::Router(
                            astersql_pd_mcs_router_server::RouterHost::start_with_log_file(
                                astersql_pd_mcs_router_server::HostConfig {
                                    runtime: tokio::runtime::Handle::current(),
                                    cancel,
                                    store,
                                    cluster_id: cluster,
                                    entry,
                                    cluster: Arc::new(std::sync::Mutex::new(
                                        astersql_pd_core::BasicCluster::new(),
                                    )),
                                    delegates,
                                },
                                listener,
                                config.log.file.filename.clone(),
                            )
                            .await?,
                        )
                    }
                    _ => Host::ResourceManager(
                        astersql_pd_rm_server::ResourceManagerHost::start_with_log_file(
                            astersql_pd_rm_server::HostConfig {
                                runtime: tokio::runtime::Handle::current(),
                                cancel,
                                store,
                                cluster_id: cluster,
                                entry,
                                controller,
                            },
                            listener,
                            config.log.file.filename.clone(),
                        )
                        .await?,
                    ),
                };
                super::entry::shutdown_signal()
                    .await
                    .map_err(|e| e.to_string())?;
                match host {
                    Host::Router(host) => host.shutdown().await,
                    Host::Scheduling(host) => host.shutdown().await,
                    Host::ResourceManager(host) => host.shutdown().await,
                }
            })
        });
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{service} startup or runtime failed: {e}");
            ExitCode::FAILURE
        }
    }
}

enum Host {
    Scheduling(astersql_pd_runtime::SchedulingHost),
    Router(astersql_pd_mcs_router_server::RouterHost),
    ResourceManager(astersql_pd_rm_server::ResourceManagerHost),
}
