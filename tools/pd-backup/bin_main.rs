// Copyright 2026 AsterSQL.

use std::process::ExitCode;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    astersql_pd_backup_cli::run(std::env::args().skip(1)).await
}
