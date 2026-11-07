// Copyright 2026 AsterSQL.

use std::process::ExitCode;

use astersql_pd_recover::{Action, USAGE, parse, recover_new_with_timestamp, recover_old};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(Action::Recover(options)) => options,
        Ok(Action::NativeRestore {
            archive,
            directory,
            cluster,
            node,
        }) => {
            let result = async {
                let bytes = std::fs::read(archive)?;
                astersql_pd_storage_kv::restore_native_backup(&bytes, &directory, cluster, node)
                    .await
            }
            .await;
            return report(result);
        }
        Ok(Action::Help) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Ok(Action::Version) => {
            println!("pd-recover {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}");
            return ExitCode::from(2);
        }
    };
    let result = async {
        let mut client = options.connect().await?;
        if options.from_old_member {
            recover_old(&mut client).await
        } else {
            recover_new_with_timestamp(
                &mut client,
                options.cluster_id,
                options.alloc_id,
                options.alloc_timestamp,
            )
            .await
        }
    }
    .await;
    report(result)
}

fn report(result: Result<(), Box<dyn std::error::Error + Send + Sync>>) -> ExitCode {
    match result {
        Ok(()) => {
            println!("recover success! please restart the PD cluster");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
