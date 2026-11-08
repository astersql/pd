// Copyright 2026 AsterSQL.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use astersql_pd_backup::{collect, write_backup};
use hyper::Uri;

use crate::Result;
use crate::transport::Credentials;

const DEFAULT_PD: &str = "http://127.0.0.1:2379";
const USAGE: &str = "Usage: pd-backup [-pd URL[,URL...]] [-file PATH] [-cacert PATH] [-cert PATH -key PATH]\n       [--etcd-endpoints URL[,URL...]]";
const TIMEOUT: Duration = Duration::from_secs(10);

struct Options {
    pd: String,
    etcd: Option<String>,
    file: PathBuf,
    ca: Option<PathBuf>,
    cert: Option<PathBuf>,
    key: Option<PathBuf>,
}

impl Options {
    fn parse(mut args: impl Iterator<Item = String>) -> Result<Option<Self>> {
        let mut options = Self {
            pd: DEFAULT_PD.into(),
            etcd: None,
            file: "backup.json".into(),
            ca: None,
            cert: None,
            key: None,
        };
        while let Some(arg) = args.next() {
            let flag = arg
                .strip_prefix("--")
                .or_else(|| arg.strip_prefix('-'))
                .ok_or("expected an option")?;
            let (name, inline) = flag
                .split_once('=')
                .map_or((flag, None), |(name, value)| (name, Some(value)));
            if matches!(name, "h" | "help") {
                return Ok(None);
            }
            let value = inline
                .map(str::to_owned)
                .or_else(|| args.next())
                .ok_or("missing option value")?;
            match name {
                "pd" => options.pd = value,
                "etcd-endpoints" => options.etcd = Some(value),
                "file" => options.file = value.into(),
                "cacert" => options.ca = Some(value.into()),
                "cert" => options.cert = Some(value.into()),
                "key" => options.key = Some(value.into()),
                _ => return Err("unknown backup option".into()),
            }
        }
        if options.cert.is_some() != options.key.is_some() {
            return Err("cert and key must be supplied together".into());
        }
        Ok(Some(options))
    }

    async fn backup(&self) -> Result<()> {
        let pd = endpoints(&self.pd)?;
        let etcd = endpoints(self.etcd.as_deref().unwrap_or(&self.pd))?;
        let credentials = Credentials::load(
            self.ca.as_deref(),
            self.cert.as_deref().zip(self.key.as_deref()),
        )?;
        let mut config_uri = pd[0].clone().into_parts();
        config_uri.path_and_query = Some("/pd/api/v1/config".parse()?);
        let config_uri = Uri::from_parts(config_uri)?;
        let backup = tokio::time::timeout(TIMEOUT, async {
            let config = credentials.fetch_config(config_uri).await?;
            let mut client = credentials.connect_etcd(&etcd, TIMEOUT).await?;
            collect(&mut client, config).await
        })
        .await??;

        let parent = self
            .file
            .parent()
            .filter(|path| !path.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        let mut output = tempfile::NamedTempFile::new_in(parent)?;
        write_backup(output.as_file_mut(), &backup)?;
        output.as_file().sync_all()?;
        output.persist(&self.file).map_err(|error| error.error)?;
        Ok(())
    }
}

fn endpoints(addresses: &str) -> Result<Vec<Uri>> {
    addresses
        .split(',')
        .map(|address| {
            let uri: Uri = address.parse()?;
            let valid_authority = uri
                .authority()
                .is_some_and(|authority| !authority.as_str().contains('@'));
            if !matches!(uri.scheme_str(), Some("http" | "https"))
                || !valid_authority
                || uri.path() != "/"
                || uri.query().is_some()
            {
                return Err("expected an HTTP or HTTPS endpoint URL".into());
            }
            Ok(uri)
        })
        .collect()
}

/// Run the PD backup command.
pub async fn run(args: impl Iterator<Item = String>) -> ExitCode {
    let options = match Options::parse(args) {
        Ok(Some(options)) => options,
        Ok(None) => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            eprintln!("{error}\n{USAGE}");
            return ExitCode::from(2);
        }
    };
    match options.backup().await {
        Ok(()) => {
            println!(
                "pd backup successful! dump file is: {}",
                options.file.display()
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
