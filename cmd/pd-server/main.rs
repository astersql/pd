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

pub(crate) async fn run_until(
    config: Config,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
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
            .with_listener_config(config.security)
            .with_join(config.join);
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
    println!("PD ready: {:?}", node.events());
    let signal_result =
        tokio::select! { result = shutdown => result, result = node.wait() => { result?; Ok(()) } };
    node.shutdown().await?;
    signal_result?;
    println!("PD stopped: {:?}", node.events());
    Ok(node.events())
}
/// Owns CLI, process signals and the standalone executor.
pub fn main() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args == ["--help"] || args == ["-h"] {
        println!(
            "pd-server [--config=FILE] [--config-check] [--etcd-member-endpoint=URL --etcd-member-id=ID --etcd-member-exclusive=true] --etcd-endpoints=http://127.0.0.1:2379 [--client-urls=http://127.0.0.1:2381] [--name=pd] [--join=URL] [--cert=FILE --key=FILE --cacert=FILE]\nMinimal classic PD; external etcd is a transitional dependency."
        );
        return ExitCode::SUCCESS;
    }
    let config = match Config::parse(args.into_iter()) {
        Ok(config) => config,
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
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
    for warning in &config.security.warning_msgs {
        eprintln!("warning: {warning}");
    }
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ProcessError::from)
        .and_then(|runtime| runtime.block_on(run_until(config, shutdown_signal())));
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("pd-server: {error}");
            ExitCode::FAILURE
        }
    }
}
async fn shutdown_signal() -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        tokio::select! { result=tokio::signal::ctrl_c()=>result, _=terminate.recv()=>Ok(()) }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
