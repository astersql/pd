// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use prost::Message;
use serde_json::json;
use std::{path::PathBuf, time::Duration};
struct Work(PathBuf);
impl Drop for Work {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_http_listener_keeps_valid_outbound_tls_configuration() {
    let work = Work(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("sa016-http-tls-{}", std::process::id())),
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    assert!(
        std::process::Command::new("python3")
            .arg(root.join("scripts/service-auth-oracle.py"))
            .arg("--certificates")
            .arg(&work.0)
            .status()
            .unwrap()
            .success()
    );
    let config = work.0.join("http.toml");
    let cert = work.0.join("server.pem").display().to_string();
    let key = work.0.join("server.key").display().to_string();
    let ca = work.0.join("ca.pem").display().to_string();
    std::fs::write(
        &config,
        format!(
            "[security]\ncacert-path={}\ncert-path={}\nkey-path={}\n",
            json!(ca),
            json!(cert),
            json!(key)
        ),
    )
    .unwrap();
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let arg = format!("--config={}", config.display());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut server = if is_rust {
            Server::start_rust_with_args(binary, &[&arg])
        } else {
            Server::start_with_args(binary, &[&arg])
        }
        .unwrap();
        match server.ready_with_timeout(Duration::from_secs(15)) {
            Err(error) => records
                .push(json!({"rust":is_rust,"ready":false,"error":error,"logs":server.logs()})),
            Ok(()) => {
                let mut client = PdClient::connect(server.url.clone()).await.unwrap();
                let members = client
                    .get_members(pdpb::GetMembersRequest::default())
                    .await
                    .unwrap()
                    .into_inner();
                let (status, config) = server.get("/pd/api/v1/config").unwrap();
                let security = &config["security"];
                let observed = json!({"http_status":status,"grpc_header_valid":members.header.as_ref().is_some_and(|h|h.cluster_id!=0&&h.error.is_none()),"ca":security["cacert-path"],"cert":security["cert-path"],"key":security["key-path"]});
                records.push(json!({"rust":is_rust,"ready":true,"observed":observed,"members_protobuf":members.encode_to_vec()}));
                server.stop().unwrap();
            }
        }
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("http-tls-config.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    assert_eq!(
        records[0]["ready"], true,
        "Go oracle must accept valid TLS credentials on HTTP listener: {}",
        records[0]
    );
    assert_eq!(
        records[0]["observed"],
        json!({"http_status":200,"grpc_header_valid":true,"ca":ca,"cert":cert,"key":key})
    );
    assert_eq!(
        records[1]["ready"], true,
        "Rust startup differs: {}",
        records[1]
    );
    assert_eq!(records[1]["observed"], records[0]["observed"]);
}
