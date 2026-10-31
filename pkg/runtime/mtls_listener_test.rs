// Copyright 2026 AsterSQL.
use crate::{
    NodeConfig, PdNodeBuilder,
    tls_listener_test::{Certificates, http},
};
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Read};
use std::{sync::Arc, time::Duration};
use tokio_util::sync::CancellationToken;

async fn rpc(
    c: &Certificates,
    url: &str,
    identity: Option<&str>,
) -> Result<u64, Box<dyn std::error::Error + Send + Sync>> {
    use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
    let mut tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(Certificate::from_pem(std::fs::read(c.path("ca.pem"))?));
    if let Some(name) = identity {
        tls = tls.identity(Identity::from_pem(
            std::fs::read(c.path(&format!("{name}.pem")))?,
            std::fs::read(c.path(&format!("{name}.key")))?,
        ));
    }
    let channel = Endpoint::from_shared(url.to_owned())?
        .tls_config(tls)?
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(3))
        .connect()
        .await?;
    let mut client = PdClient::new(channel);
    let members = client
        .get_members(pdpb::GetMembersRequest::default())
        .await?
        .into_inner();
    let id = client
        .alloc_id(pdpb::AllocIdRequest {
            header: Some(pdpb::RequestHeader {
                cluster_id: members.header.unwrap().cluster_id,
                ..Default::default()
            }),
            ..Default::default()
        })
        .await?
        .into_inner();
    assert!(id.header.unwrap().error.is_none());
    Ok(id.id)
}
fn no_identity(c: &Certificates, url: &str) -> bool {
    std::process::Command::new("curl")
        .args([
            "-fsS",
            "--max-time",
            "3",
            "--cacert",
            &c.path("ca.pem"),
            &format!("{url}/pd/api/v1/ping"),
        ])
        .output()
        .unwrap()
        .status
        .success()
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_084_behavior() {
    let c = Certificates::new();
    assert!(
        std::process::Command::new("python3")
            .args([
                "../../scripts/mtls-listener-oracle.py",
                "--expired",
                &c.path("")
            ])
            .status()
            .unwrap()
            .success()
    );
    // Classic Go enables CN filtering only for lists with more than one element.
    for allowed in [vec!["allowed", "server"], vec!["server"], vec![]] {
        let fixture = crate::node_test::fixture::Fixture::start();
        let store = Arc::new(fixture.store().await);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let url = format!("https://{addr}");
        let mut config = c.config();
        config.security.tls.cert_allowed_c_ns = Some(allowed.iter().map(|s| (*s).into()).collect());
        let cancel = CancellationToken::new();
        let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), cancel.clone())
            .with_listener_config(config)
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 84,
                    member_id: 84,
                    name: "mtls".into(),
                    client_url: url.clone(),
                    peer_url: "http://peer:2380".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
        node.ready().await.unwrap();
        let success = http(&c, &url, "ca.pem", "allowed");
        assert!(success.0);
        assert_eq!(success.1, 200);
        let id = rpc(&c, &url, Some("allowed")).await.unwrap();
        assert!(id > 0);
        let persisted = store
            .read(Read::prefix(b"/pd/84/alloc_id", Consistency::Linearizable))
            .await
            .unwrap();
        assert!(
            u64::from_be_bytes(persisted.entries[0].value.as_slice().try_into().unwrap()) >= id
        );
        let restricted = allowed.len() > 1;
        let before_cn = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap();
        assert_eq!(http(&c, &url, "ca.pem", "denied").0, !restricted);
        assert_eq!(rpc(&c, &url, Some("denied")).await.is_ok(), !restricted);
        if restricted {
            assert_eq!(
                before_cn.entries,
                store
                    .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                    .await
                    .unwrap()
                    .entries
            );
        }
        let before = store
            .read(Read::prefix(b"/pd/", Consistency::Linearizable))
            .await
            .unwrap();
        for name in ["expired", "rogue"] {
            assert!(!http(&c, &url, "ca.pem", name).0, "HTTP {name}");
            assert!(rpc(&c, &url, Some(name)).await.is_err(), "RPC {name}");
        }
        assert!(!no_identity(&c, &url));
        assert!(rpc(&c, &url, None).await.is_err());
        assert_eq!(
            before.entries,
            store
                .read(Read::prefix(b"/pd/", Consistency::Linearizable))
                .await
                .unwrap()
                .entries
        );
        if let Ok(urls) = std::env::var("PD_MTLS_GO_URLS") {
            let urls: Vec<String> = serde_json::from_str(&urls).unwrap();
            let go = &urls[if restricted {
                0
            } else if allowed.is_empty() {
                2
            } else {
                1
            }];
            assert_eq!(success, http(&c, go, "ca.pem", "allowed"));
            assert!(rpc(&c, go, Some("allowed")).await.unwrap() > 0);
            assert_eq!(http(&c, go, "ca.pem", "denied").0, !restricted);
            assert_eq!(rpc(&c, go, Some("denied")).await.is_ok(), !restricted);
            for name in ["expired", "rogue"] {
                assert!(!http(&c, go, "ca.pem", name).0);
                assert!(rpc(&c, go, Some(name)).await.is_err());
            }
            assert!(!no_identity(&c, go));
            assert!(rpc(&c, go, None).await.is_err());
            eprintln!(
                "task084 Go/Rust differential CN={allowed:?}: HTTP200/h2 AllocID; expired/rogue/missing rejected"
            );
        }
        eprintln!(
            "task084 CN={allowed:?}: HTTP={success:?}, AllocID={id}; rejected requests preserve PD entries"
        );
        let mut stalled = tokio::net::TcpStream::connect(addr).await.unwrap();
        cancel.cancel();
        tokio::time::timeout(Duration::from_secs(5), node.shutdown())
            .await
            .unwrap()
            .unwrap();
        use tokio::io::AsyncReadExt;
        assert!(
            tokio::time::timeout(Duration::from_secs(2), stalled.read_u8())
                .await
                .unwrap()
                .is_err()
        );
        tokio::net::TcpListener::bind(addr).await.unwrap();
    }
}
