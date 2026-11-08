// Copyright 2026 AsterSQL.
//! Process argument syntax and informational output, before runtime creation.
pub(crate) const HELP: &str = r"Placement Driver server (Rust)

Usage: pd-server [flags]

Flags:
  -h, --help                     show help and exit
  -V, --version                  print version information and exit
      --config=FILE              read TOML configuration
      --config-check             validate configuration and exit
      --name=NAME                member name
      --client-urls=URL          client listener
      --peer-urls=URL            peer address
      --advertise-client-urls=URL
      --advertise-peer-urls=URL
      --join=URL                 join existing cluster
      --data-dir=DIR             embedded backend setting (startup unsupported)
      --initial-cluster=MEMBERS  embedded backend setting (startup unsupported)
      --force-new-cluster        embedded backend setting (startup unsupported)
  -L, --log-level=LEVEL
      --log-file=FILE
      --metrics-addr=URL
      --cacert=FILE --cert=FILE --key=FILE
      --etcd-endpoints=URLS      comma-separated external etcd endpoints (default backend)
      --etcd-member-endpoint=URL --etcd-member-id=ID
      --etcd-member-exclusive=true
      --local-native-data-dir=DIR
      --local-native-cluster-id=ID

Default startup requires external etcd. Supply both local-native flags for
isolated, loopback-only, single-voter SQLite/OpenRaft development.
Help, version and config-check do not open a store.
";

pub(crate) struct Arguments {
    pub config: Vec<String>,
    pub help: bool,
    pub version: bool,
}
pub(crate) fn parse(args: Vec<String>) -> Result<Arguments, String> {
    let mut result = Arguments {
        config: Vec::new(),
        help: false,
        version: false,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        if arg == "help" {
            result.help = true;
            continue;
        }
        let (key, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(k, v)| (k, Some(v)));
        let boolean = matches!(key, "--help" | "-h" | "--version" | "-V")
            || crate::cli_config::BOOLEAN_FLAGS.contains(&key);
        if !boolean && key != "-L" && !crate::cli_config::STRING_FLAGS.contains(&key) {
            return Err(format!("unknown option: {key}"));
        }
        let value = if boolean {
            let value = inline.unwrap_or("true");
            crate::cli_config::boolean(value).map_err(|e| e.to_string())?;
            value.to_owned()
        } else {
            inline
                .map(str::to_owned)
                .or_else(|| args.next())
                .ok_or_else(|| format!("flag needs an argument: {key}"))?
        };
        match key {
            "--help" | "-h" => result.help = crate::cli_config::boolean(&value).unwrap(),
            "--version" | "-V" => result.version = crate::cli_config::boolean(&value).unwrap(),
            _ => result.config.push(format!("{key}={value}")),
        }
    }
    Ok(result)
}
pub(crate) fn version(local_native: bool) -> String {
    format!(
        "Release Version: {}\nEdition: Community\nKernel Type: Classic\nGit Commit Hash: {}\nGit Branch: {}\nUTC Build Time:  {}\nImplementation: Rust ({})",
        env!("CARGO_PKG_VERSION"),
        option_env!("PD_GIT_HASH").unwrap_or("unknown"),
        option_env!("PD_GIT_BRANCH").unwrap_or("unknown"),
        option_env!("PD_BUILD_TIME").unwrap_or("unknown"),
        if local_native {
            "local native MetaStore"
        } else {
            "external etcd MetaStore"
        },
    )
}

pub(crate) fn process_args() -> Vec<String> {
    #[cfg(test)]
    if let Ok(args) = std::env::var("PD_TASK091_CHILD_ARGS") {
        return if args.is_empty() {
            Vec::new()
        } else {
            args.split('\u{1f}').map(str::to_owned).collect()
        };
    }
    std::env::args().skip(1).collect()
}
