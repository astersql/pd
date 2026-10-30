// Copyright 2026 AsterSQL.
use astersql_pd_runtime::PdNodeBuilder;
use astersql_pd_storage::{EtcdMetaStore, MetaStore};
use std::{error::Error, future::Future, process::ExitCode, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
type ProcessError = Box<dyn Error + Send + Sync>;

pub(crate) struct Config {
    pub endpoint: String,
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
        let mut config = Self {
            endpoint: String::new(),
            diagnostic_endpoint: None,
            diagnostic_member: None,
            dedicated_member: false,
            client: "http://127.0.0.1:2379".into(),
            peer: "http://127.0.0.1:2380".into(),
            name: "pd".into(),
            advertise_client: None,
            advertise_peer: None,
        };
        for arg in args {
            let (key, value) = arg.split_once('=').ok_or("expected --option=value")?;
            match key {
                "--etcd-endpoints" => config.endpoint = value.into(),
                "--etcd-member-endpoint" => config.diagnostic_endpoint = Some(value.into()),
                "--etcd-member-exclusive" => config.dedicated_member = value.parse()?,
                "--etcd-member-id" => config.diagnostic_member = Some(value.parse()?),
                "--client-urls" => config.client = value.into(),
                "--peer-urls" => config.peer = value.into(),
                "--name" => config.name = value.into(),
                "--advertise-client-urls" => config.advertise_client = Some(value.into()),
                "--advertise-peer-urls" => config.advertise_peer = Some(value.into()),
                // Compatibility harness supplies Go process metadata. No local storage is opened.
                "--data-dir" | "--initial-cluster" => {}
                _ => return Err(format!("unsupported option: {key}").into()),
            }
        }
        if config.endpoint.is_empty() {
            return Err("--etcd-endpoints is required for the transitional backend".into());
        }
        if config.diagnostic_endpoint.is_some() != config.diagnostic_member.is_some()
            || config.diagnostic_member == Some(0)
        {
            return Err(
                "--etcd-member-endpoint and nonzero --etcd-member-id must be supplied together"
                    .into(),
            );
        }
        if config.dedicated_member && config.diagnostic_member.is_none() {
            return Err(
                "exclusive ownership requires an explicit etcd member endpoint and ID".into(),
            );
        }
        Ok(config)
    }
}
pub(crate) async fn run_until(
    config: Config,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
) -> Result<Vec<&'static str>, ProcessError> {
    use astersql_pd_runtime::net_address::Endpoint;
    let endpoint = Endpoint::parse(&config.client)?;
    let advertised = Endpoint::parse(config.advertise_client.as_deref().unwrap_or(&config.client))?;
    let peer = Endpoint::parse(config.advertise_peer.as_deref().unwrap_or(&config.peer))?.url();
    Endpoint::parse(&config.peer)?;
    let listener = endpoint.bind().await?;
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
    let mut builder =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new());
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
            "pd-server [--etcd-member-endpoint=URL --etcd-member-id=ID --etcd-member-exclusive=true] --etcd-endpoints=http://127.0.0.1:2379 [--client-urls=http://127.0.0.1:2381] [--name=pd]\nMinimal classic PD; external etcd is a transitional dependency."
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
