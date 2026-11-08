// Copyright 2026 AsterSQL.
#[cfg(all(test, unix))]
mod classic_process;

#[cfg(all(test, unix))]
mod cli_config;

#[cfg(all(test, unix))]
mod native_process_test_support;

#[cfg(all(test, unix))]
mod native_standalone_test;

fn main() -> std::process::ExitCode {
    astersql_pd_cmd_pd_server::main()
}
