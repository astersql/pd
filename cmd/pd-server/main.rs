// Copyright 2026 AsterSQL.
use astersql_pd_runtime::PdNodeBuilder;
use astersql_pd_storage::{EtcdMetaStore, MetaStore};
use std::{error::Error, future::Future, process::ExitCode, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
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
impl Config {
    pub(crate) fn parse(args: impl Iterator<Item = String>) -> Result<Self, ProcessError> {
        crate::cli_config::parse(args)
    }
}

#[cfg(test)]
pub(crate) async fn run_until(
    config: Config,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
) -> Result<Vec<&'static str>, ProcessError> {
    run_with_logging(config, shutdown, None).await
}
async fn run_with_logging(
    config: Config,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
    log_level: Option<Arc<dyn astersql_pd_logutil::LogLevelControl>>,
) -> Result<Vec<&'static str>, ProcessError> {
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
    let store: Arc<dyn MetaStore> = Arc::new(
        EtcdMetaStore::connect(
            &config
                .endpoint
                .split(',')
                .map(String::from)
                .collect::<Vec<_>>(),
            Duration::from_secs(2),
        )
        .await?,
    );
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
/// Owns CLI, process signals and the standalone executor.
pub fn main() -> ExitCode {
    let raw = std::env::args().skip(1).collect::<Vec<_>>();
    if raw.first().is_some_and(|s| s == "services") && raw.get(1).is_some_and(|s| s == "tso") {
        return crate::standalone_tso::main(raw[2..].to_vec());
    }

    if raw.first().is_some_and(|s| s == "services")
        && raw
            .get(1)
            .is_some_and(|s| matches!(s.as_str(), "resource-manager" | "router" | "scheduling"))
    {
        return crate::standalone_mcs::main(&raw[1], raw[2..].to_vec());
    }
    let args = match crate::cli_exit::parse(crate::cli_exit::process_args()) {
        Ok(args) => args,
        Err(error) => {
            println!("Error: {error}\n{}", crate::cli_exit::HELP);
            return ExitCode::FAILURE;
        }
    };
    if args.help {
        print!("{}", crate::cli_exit::HELP);
        return ExitCode::SUCCESS;
    }
    let config = match Config::parse(args.config.into_iter()) {
        Ok(config) => config,
        Err(error) => {
            println!("{}", crate::cli_logging::config_error(&error.to_string()));
            return ExitCode::SUCCESS;
        }
    };
    if args.version {
        println!("{}", crate::cli_exit::version());
        return ExitCode::SUCCESS;
    }
    if config.config_check {
        if config.security.warning_msgs.is_empty() {
            println!("config check successful");
        } else {
            for warning in &config.security.warning_msgs {
                println!("{warning}");
            }
        }
        return ExitCode::SUCCESS;
    }
    if config.endpoint.is_empty() {
        eprintln!("pd-server: --etcd-endpoints is required for the transitional backend");
        return ExitCode::FAILURE;
    }
    let log_level = match crate::cli_logging::init(&config.security.log) {
        Ok(control) => control,
        Err(_) => {
            println!("pd-server: log initialization failed");
            return ExitCode::FAILURE;
        }
    };
    for _ in &config.security.warning_msgs {
        tracing::warn!(target: "pd", "deprecated configuration option");
    }
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ProcessError::from)
        .and_then(|runtime| {
            runtime.block_on(run_with_logging(config, shutdown_signal(), Some(log_level)))
        });
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            // Backend errors can embed credential-bearing endpoints or config.
            // Keep only a stable classification at the process logging boundary.
            if error
                .to_string()
                .to_ascii_lowercase()
                .contains("address already in use")
            {
                tracing::error!(target: "pd", "pd-server: Address already in use");
            } else {
                tracing::error!(target: "pd", "pd-server: startup or runtime failed");
            }
            ExitCode::FAILURE
        }
    }
}
pub(crate) async fn shutdown_signal() -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate())?;
        let mut hangup = signal(SignalKind::hangup())?;
        let mut quit = signal(SignalKind::quit())?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => {
                result?;
                Err(std::io::Error::other("interrupted by SIGINT"))
            }
            _ = terminate.recv() => Ok(()),
            _ = hangup.recv() => Err(std::io::Error::other("interrupted by SIGHUP")),
            _ = quit.recv() => Err(std::io::Error::other("interrupted by SIGQUIT")),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c().await?;
        Err(std::io::Error::other("interrupted by SIGINT"))
    }
}
