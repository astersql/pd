// Copyright 2026 AsterSQL.
use std::{future::Future, process::ExitCode, sync::Arc, time::Duration};

use astersql_pd_storage::{EtcdMetaStore, MetaStore};

use crate::classic_process::run_with_backend;
pub(crate) use crate::classic_process::{Config, ProcessError, prepare_shutdown_signal};

async fn shutdown_signal() -> Result<(), std::io::Error> {
    prepare_shutdown_signal()?.await
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
    run_with_backend(config, shutdown, log_level, |endpoint| async move {
        let endpoints: Vec<_> = endpoint.split(',').map(String::from).collect();
        let store: Arc<dyn MetaStore> =
            Arc::new(EtcdMetaStore::connect(&endpoints, Duration::from_secs(2)).await?);
        Ok(store)
    })
    .await
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
