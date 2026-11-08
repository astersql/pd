// Copyright 2026 AsterSQL.
//! Classic PD process assembly with a caller-provided metadata connector.

use std::{error::Error, future::Future, sync::Arc, time::Duration};

use tokio_util::sync::CancellationToken;

use astersql_pd_runtime::PdNodeBuilder;
use astersql_pd_storage::MetaStore;

pub(crate) type ProcessError = Box<dyn Error + Send + Sync>;

pub(crate) struct Config {
    pub config_check: bool,
    pub unsupported: Vec<String>,
    pub endpoint: String,
    pub join: String,
    pub security: astersql_pd_server_config::Config,
    pub diagnostic_endpoint: Option<String>,
    pub diagnostic_member: Option<u64>,
    pub dedicated_member: bool,
    pub client: String,
    pub peer: String,
    pub name: String,
    pub advertise_client: Option<String>,
    pub advertise_peer: Option<String>,
}
pub(crate) async fn run_with_backend<OpenFuture>(
    config: Config,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
    log_level: Option<Arc<dyn astersql_pd_logutil::LogLevelControl>>,
    open_store: impl FnOnce(String) -> OpenFuture,
) -> Result<Vec<&'static str>, ProcessError>
where
    OpenFuture: Future<Output = Result<Arc<dyn MetaStore>, ProcessError>>,
{
    if !config.unsupported.is_empty() {
        return Err(format!(
            "external MetaStore backend cannot apply: {}",
            config.unsupported.join(", ")
        )
        .into());
    }
    use astersql_pd_runtime::net_address::Endpoint;
    let endpoint = Endpoint::parse(&config.client)?;
    let advertised = Endpoint::parse(config.advertise_client.as_deref().unwrap_or(&config.client))?;
    let peer = Endpoint::parse(config.advertise_peer.as_deref().unwrap_or(&config.peer))?.url();
    Endpoint::parse(&config.peer)?;
    let listener = endpoint.bind().await?;
    if endpoint.url().starts_with("https://") != advertised.url().starts_with("https://") {
        return Err("listener and advertised client schemes must match".into());
    }
    let client = advertised.advertised_url(listener.local_addr()?)?;
    let store = open_store(config.endpoint).await?;
    // A fresh join must use an existing external cluster, never bootstrap a
    // replacement identity in an empty or misconfigured MetaStore.
    if !config.join.is_empty() {
        if config.join == client {
            return Err("join self is forbidden".into());
        }
        use astersql_pd_storage::{Consistency, Read};
        if store
            .read(Read::range(
                b"/pd/cluster_id",
                b"",
                Consistency::Linearizable,
            ))
            .await?
            .entries
            .is_empty()
        {
            return Err("join requires an existing external MetaStore cluster identity".into());
        }
    }
    let mut builder =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .with_keyspace_groups(
                std::env::var("PD_SERVICE_MODE").is_ok_and(|s| s.eq_ignore_ascii_case("api")),
            )
            .with_resource_manager_fallback(
                config
                    .security
                    .microservice
                    .is_resource_manager_fallback_enabled(),
            )
            .with_startup_config(config.security.clone())
            .with_listener_config(config.security)
            .with_join(config.join);
    if let Some(control) = log_level {
        builder = builder.with_log_level_control(control);
    }
    if let (Some(endpoint), Some(member)) = (config.diagnostic_endpoint, config.diagnostic_member) {
        let diagnostics = astersql_pd_runtime::etcd_diagnostics::EtcdMemberDiagnostics::connect(
            endpoint.clone(),
            member,
            Duration::from_secs(2),
        )
        .await?;
        diagnostics.validate_store(store.as_ref()).await?;
        builder = builder.with_etcd_member_diagnostics(Arc::new(diagnostics));
        if config.dedicated_member {
            let admin = astersql_pd_runtime::member_admin::EtcdMemberAdmin::connect(
                endpoint,
                member,
                Duration::from_secs(2),
            )
            .await?;
            builder = builder.with_etcd_member_admin(Arc::new(admin));
        }
    }
    let identity =
        astersql_pd_runtime::bootstrap_config(store.as_ref(), config.name, client, peer).await?;
    let mut node = builder
        .start(
            store,
            identity,
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await?;
    node.ready().await?;
    tracing::info!(target: "pd", "PD ready");
    let signal_result =
        tokio::select! { result = shutdown => result, result = node.wait() => { result?; Ok(()) } };
    node.shutdown().await?;
    signal_result?;
    tracing::info!(target: "pd", "PD stopped");
    Ok(node.events())
}
/// Install process signals before publishing listeners; the caller decides when
/// to await them. Notifications received during startup remain buffered.
pub(crate) fn prepare_shutdown_signal()
-> Result<impl std::future::Future<Output = Result<(), std::io::Error>>, std::io::Error> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate())?;
        let mut interrupt = signal(SignalKind::interrupt())?;
        let mut hangup = signal(SignalKind::hangup())?;
        let mut quit = signal(SignalKind::quit())?;
        Ok(async move {
            tokio::select! {
                _=interrupt.recv()=>Err(std::io::Error::other("interrupted by SIGINT")),
                _=terminate.recv()=>Ok(()),
                _=hangup.recv()=>Err(std::io::Error::other("interrupted by SIGHUP")),
                _=quit.recv()=>Err(std::io::Error::other("interrupted by SIGQUIT")),
            }
        })
    }
    #[cfg(not(unix))]
    {
        Ok(async {
            tokio::signal::ctrl_c().await?;
            Err(std::io::Error::other("interrupted by SIGINT"))
        })
    }
}
