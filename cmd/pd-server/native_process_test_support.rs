// Copyright 2026 AsterSQL.

use std::{
    fs::{self, File},
    net::SocketAddr,
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use astersql_pd_proto::pdpb::{self, pd_client::PdClient};

pub(super) struct Host {
    pub(super) child: Child,
    log: PathBuf,
}

impl Host {
    pub(super) fn start(
        root: &Path,
        label: &str,
        address: Option<SocketAddr>,
        fixture: &str,
    ) -> Self {
        let log = root.join(format!("{label}.log"));
        let output = File::create(&log).unwrap();
        let ready = root.join(format!("{label}.ready"));
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--ignored", "--exact", fixture, "--nocapture"])
            .env_clear()
            .env("PATH", "/nonexistent")
            .env("PD_NATIVE_HOST_STORE", root.join("store"))
            .env("PD_NATIVE_HOST_READY", &ready)
            .env(
                "PD_NATIVE_HOST_ADDRESS",
                address.map(|a| a.to_string()).unwrap_or_default(),
            )
            .stdin(Stdio::null())
            .stdout(output.try_clone().unwrap())
            .stderr(output)
            .spawn()
            .unwrap();
        Self { child, log }
    }

    pub(super) fn logs(&self) -> String {
        fs::read_to_string(&self.log).unwrap()
    }

    pub(super) async fn wait_for(&mut self, observe: impl Fn(&Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !observe(self) {
            assert!(self.child.try_wait().unwrap().is_none(), "{}", self.logs());
            assert!(Instant::now() < deadline, "{}", self.logs());
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }

    pub(super) async fn wait(&mut self) -> ExitStatus {
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    return status;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("{}", self.logs()))
    }

    pub(super) fn signal(&self, signal: &str) {
        assert!(
            Command::new("/bin/kill")
                .args([signal, &self.child.id().to_string()])
                .status()
                .unwrap()
                .success()
        );
    }

    pub(super) fn assert_no_children(&self) {
        let output = Command::new("/bin/ps")
            .args(["-axo", "ppid="])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert!(
            !String::from_utf8(output.stdout)
                .unwrap()
                .lines()
                .any(|id| id.trim().parse::<u32>() == Ok(self.child.id()))
        );
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}

pub(super) async fn observe(address: SocketAddr) -> (u64, u64, (i64, i64)) {
    let endpoint = tonic::transport::Endpoint::from_shared(format!("http://{address}"))
        .unwrap()
        .timeout(Duration::from_secs(2));
    let mut client = PdClient::new(endpoint.connect_lazy());
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner();
    let cluster = members.header.unwrap().cluster_id;
    assert_ne!(cluster, 0);
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let deadline = Instant::now() + Duration::from_secs(12);
    let id = loop {
        let response = client
            .alloc_id(pdpb::AllocIdRequest {
                header: header.clone(),
                ..Default::default()
            })
            .await;
        if let Ok(response) = response {
            let response = response.into_inner();
            if response.header.is_some_and(|h| h.error.is_none()) && response.id > 0 {
                break response.id;
            }
        }
        assert!(
            Instant::now() < deadline,
            "native host did not acquire the PD term"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let mut tso = client
        .tso(tokio_stream::iter([pdpb::TsoRequest {
            header,
            count: 1,
            ..Default::default()
        }]))
        .await
        .unwrap()
        .into_inner();
    let tso = tso.message().await.unwrap().unwrap();
    assert!(tso.header.unwrap().error.is_none());
    let ts = tso.timestamp.unwrap();
    let response = Command::new("/usr/bin/curl")
        .args(["-fsS", "--max-time", "3"])
        .arg(format!("http://{address}/pd/api/v1/health"))
        .output()
        .unwrap();
    assert!(response.status.success());
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&response.stdout).unwrap()[0]["health"],
        true
    );
    (cluster, id, (ts.physical, ts.logical))
}
