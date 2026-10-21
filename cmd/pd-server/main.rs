// Copyright 2026 AsterSQL.
use std::{error::Error, future::Future, process::ExitCode};

use astersql_pd_runtime::PdNodeBuilder;
use tokio_util::sync::CancellationToken;

type ProcessError = Box<dyn Error + Send + Sync>;

pub(crate) async fn run_until(
    shutdown: impl Future<Output = Result<(), std::io::Error>>,
) -> Result<(), ProcessError> {
    let mut node =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new()).start();
    node.ready().await?;
    println!("Rust PD lifecycle shell started (no PD services)");
    let signal_result = shutdown.await;
    node.shutdown().await?;
    signal_result?;
    println!("Rust PD lifecycle shell stopped");
    Ok(())
}

/// Runs the standalone shell, owning CLI, signals, and the Tokio runtime.
pub fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() == 1 && (args[0] == "--help" || args[0] == "-h") {
        println!("pd-server [--help]\nRust lifecycle shell only; PD services are not implemented.");
        return ExitCode::SUCCESS;
    }
    if !args.is_empty() {
        eprintln!("unsupported argument; use --help");
        return ExitCode::from(2);
    }
    let result = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(ProcessError::from)
        .and_then(|runtime| runtime.block_on(run_until(shutdown_signal())));
    match result {
        Ok(()) => ExitCode::SUCCESS,
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
        tokio::select! {
            result = tokio::signal::ctrl_c() => result,
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c().await
}
