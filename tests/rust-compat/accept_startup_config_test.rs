// Copyright 2026 AsterSQL.
use super::accept_extra_transport_config_test::Work;
use astersql_pd_storage_kv::{Consistency, EtcdMetaStore, MetaStore, Read};
use serde_json::json;
use std::{path::PathBuf, time::Duration};
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_pd_startup_file_config_and_absent_document_restart() {
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let work = Work(std::env::temp_dir().join(format!("pd-startup-config-{}", std::process::id())));
    std::fs::create_dir(&work.0).unwrap();
    let config = work.0.join("pd.toml");
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        std::fs::write(
            &config,
            "[replication]\nmax-replicas=2\nenable-placement-rules=false\n",
        )
        .unwrap();
        let flag = format!("--config={}", config.display());
        let environment = [("PD_SERVICE_MODE".into(), "api".into())];
        let mut owner = if is_rust {
            crate::harness::Server::start_rust_with_environment(binary, &[&flag], &environment)
        } else {
            crate::harness::Server::start_with_environment(binary, &[&flag], &environment)
        }
        .unwrap();
        owner.ready().unwrap();
        let initial = owner.get("/pd/api/v1/config").unwrap();
        let mut client = astersql_pd_proto::pdpb::pd_client::PdClient::connect(owner.url.clone())
            .await
            .unwrap();
        let cluster = client
            .get_members(astersql_pd_proto::pdpb::GetMembersRequest::default())
            .await
            .unwrap()
            .into_inner()
            .header
            .unwrap()
            .cluster_id;
        drop(client);
        let backend =
            EtcdMetaStore::connect(&[owner.metadata_url().into()], Duration::from_secs(2))
                .await
                .unwrap();
        let key = astersql_pd_keypath::ClusterPaths::new(cluster).config_path();
        let before = backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        std::fs::write(
            &config,
            "[replication]\nmax-replicas=4\nenable-placement-rules=true\n",
        )
        .unwrap();
        let endpoint = format!("--etcd-endpoints={}", owner.metadata_url());
        let args = if is_rust {
            vec![flag.as_str(), endpoint.as_str()]
        } else {
            vec![flag.as_str()]
        };
        owner.restart_with_args(binary, &args).unwrap();
        owner.ready().unwrap();
        let restored = owner.get("/pd/api/v1/config").unwrap();
        let after = backend
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap();
        let row = json!({"rust":is_rust,"initial":initial,"restored":restored,"config_key":key,"before":before.entries.iter().map(|e|json!({"value":e.value,"mod_revision":e.mod_revision})).collect::<Vec<_>>(),"after":after.entries.iter().map(|e|json!({"value":e.value,"mod_revision":e.mod_revision})).collect::<Vec<_>>(),"logs":owner.logs()});
        records.push(row);
        std::fs::write(
            PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
                .with_extension("startup-config.json"),
            serde_json::to_vec(&records).unwrap(),
        )
        .unwrap();
        drop(backend);
        owner.stop().unwrap();
    }
    for row in &records {
        for phase in ["initial", "restored"] {
            assert_eq!(row[phase][0], 200, "{row}");
            assert_eq!(
                row[phase][1]["replication"]["max-replicas"],
                if phase == "initial" { 2 } else { 4 },
                "{row}"
            );
            assert_eq!(
                row[phase][1]["replication"]["enable-placement-rules"],
                if phase == "initial" { "false" } else { "true" },
                "{row}"
            );
        }
        assert_eq!(row["before"], json!([]), "{row}");
        assert_eq!(row["after"], json!([]), "{row}");
    }
}
