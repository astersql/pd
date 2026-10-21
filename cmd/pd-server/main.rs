// Copyright 2026 AsterSQL.
use astersql_pd_runtime::PdNodeBuilder;
use astersql_pd_storage::{EtcdMetaStore, MetaStore};
use std::{error::Error, future::Future, process::ExitCode, sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;
type ProcessError = Box<dyn Error + Send + Sync>;

pub(crate) struct Config {
    pub endpoint: String,
    pub client: String,
    pub peer: String,
    pub name: String,
}
impl Config {
    fn parse(args: impl Iterator<Item = String>) -> Result<Self, ProcessError> {
        let mut config = Self {
            endpoint: String::new(),
            client: "http://127.0.0.1:2379".into(),
            peer: "http://127.0.0.1:2380".into(),
            name: "pd".into(),
        };
        for arg in args {
            let (key, value) = arg.split_once('=').ok_or("expected --option=value")?;
            match key {
                "--etcd-endpoints" => config.endpoint = value.into(),
                "--client-urls" => config.client = value.into(),
                "--peer-urls" => config.peer = value.into(),
                "--name" => config.name = value.into(),
                // Compatibility harness supplies Go process metadata. No local storage is opened.
                "--data-dir"
                | "--advertise-client-urls"
                | "--advertise-peer-urls"
                | "--initial-cluster" => {}
                _ => return Err(format!("unsupported option: {key}").into()),
            }
        }
        if config.endpoint.is_empty() {
            return Err("--etcd-endpoints is required for the transitional backend".into());
        }
        Ok(config)
    }
}
pub(crate) async fn run_until(
    config: Config,
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
) -> Result<Vec<&'static str>, ProcessError> {
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
    let identity = astersql_pd_runtime::bootstrap_config(
        store.as_ref(),
        config.name,
        config.client.clone(),
        config.peer,
    )
    .await?;
    let address = config
        .client
        .strip_prefix("http://")
        .ok_or("only http:// listeners are supported by the MVP")?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
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
            "pd-server --etcd-endpoints=http://127.0.0.1:2379 [--client-urls=http://127.0.0.1:2381] [--name=pd]\nMinimal classic PD; external etcd is a transitional dependency."
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
