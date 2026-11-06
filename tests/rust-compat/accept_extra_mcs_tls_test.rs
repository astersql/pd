// Copyright 2026 AsterSQL.
use super::{
    accept_extra_rm_service_test::Sidecar, accept_extra_transport_config_test::certificates,
};
use crate::harness::Server;
use astersql_pd_proto::health::{HealthCheckRequest, health_client::HealthClient};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use tonic::transport::{Certificate, ClientTlsConfig, Endpoint, Identity};
async fn probe(url: &str, certs: &Path, client: Option<&str>) -> Value {
    let mut tls = ClientTlsConfig::new()
        .domain_name("localhost")
        .ca_certificate(Certificate::from_pem(
            std::fs::read(certs.join("ca.pem")).unwrap(),
        ));
    if let Some(client) = client {
        tls = tls.identity(Identity::from_pem(
            std::fs::read(certs.join(format!("{client}.pem"))).unwrap(),
            std::fs::read(certs.join(format!("{client}.key"))).unwrap(),
        ));
    }
    let endpoint = Endpoint::from_shared(url.replacen("http://", "https://", 1))
        .unwrap()
        .tls_config(tls)
        .unwrap()
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(2));
    let response = tokio::time::timeout(Duration::from_secs(3), async {
        let channel = endpoint
            .connect()
            .await
            .map_err(|e| format!("connect: {e:?}"))?;
        let mut client = HealthClient::new(channel);
        client
            .check(HealthCheckRequest::default())
            .await
            .map(|r| r.into_inner().status)
            .map_err(|e| format!("rpc: {e:?}"))
    })
    .await;
    match response {
        Ok(Ok(status)) => json!({"accepted":true,"health_status":status}),
        Ok(Err(error)) => json!({"accepted":false,"error":error}),
        Err(error) => json!({"accepted":false,"error":error.to_string()}),
    }
}
pub(super) async fn ready(sidecar: &mut Sidecar, certs: &Path) -> Value {
    ready_as(sidecar, certs, Some("allowed")).await
}
async fn ready_as(sidecar: &mut Sidecar, certs: &Path, identity: Option<&str>) -> Value {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Some(status) = sidecar.child.try_wait().unwrap() {
            break json!({"accepted":false,"process_exit":status.to_string(),"logs":sidecar.logs()});
        }
        let result = probe(&sidecar.url, certs, identity).await;
        if result["accepted"] == true || Instant::now() >= deadline {
            break result;
        }
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_tls_listener_uses_credentials_optional_certificates_and_all_cn_lists() {
    let certs = certificates("mcs-tls");
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        for service in ["resource-manager", "router", "scheduling", "tso"] {
            for (policy, names) in [
                ("optional", vec![]),
                ("single", vec!["allowed"]),
                ("multiple", vec!["allowed", "server"]),
            ] {
                let cfg = certs.0.join(format!("{service}-{policy}.toml"));
                std::fs::write(&cfg,format!("[security]\ncacert-path={}\ncert-path={}\nkey-path={}\ncert-allowed-cn={}\n", json!(certs.0.join("ca.pem")),json!(certs.0.join("server.pem")),json!(certs.0.join("server.key")),json!(names))).unwrap();
                let arg = format!("--config={}", cfg.display());
                // Explicit credentials select TLS even with an advertised HTTP URI.
                let mut sidecar = Sidecar::start_service_with_args(
                    binary,
                    pd.metadata_url(),
                    service,
                    "http",
                    &[&arg],
                );
                let ready = ready(&mut sidecar, &certs.0).await;
                let mut clients = vec![];
                if ready["accepted"] == true {
                    for identity in [
                        None,
                        Some("allowed"),
                        Some("denied"),
                        Some("rogue"),
                        Some("server"),
                    ] {
                        clients.push(json!({"identity":identity,"result":probe(&sidecar.url,&certs.0,identity).await}));
                    }
                    sidecar.stop();
                }
                records.push(json!({"rust":is_rust,"service":service,"policy":policy,"cn":names,"ready":ready,"clients":clients}));
            }
        }
        pd.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("mcs-tls.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    assert_eq!(records.len(), 24);
    for n in 0..12 {
        let oracle = &records[n];
        assert_eq!(oracle["ready"]["accepted"], true, "Go startup: {oracle}");
        let expected = match oracle["policy"].as_str().unwrap() {
            "optional" => [true, true, true, false, true],
            "single" => [false, true, false, false, false],
            "multiple" => [false, true, false, false, true],
            _ => unreachable!(),
        };
        for (case, accepted) in expected.into_iter().enumerate() {
            assert_eq!(
                oracle["clients"][case]["result"]["accepted"], accepted,
                "Go case {case}: {oracle}"
            );
        }
    }
    for n in 0..12 {
        let oracle = &records[n];
        let rust = &records[n + 12];
        assert_eq!(rust["ready"]["accepted"], true, "Rust startup: {rust}");
        for case in 0..5 {
            assert_eq!(
                rust["clients"][case]["identity"],
                oracle["clients"][case]["identity"]
            );
            assert_eq!(
                rust["clients"][case]["result"]["accepted"],
                oracle["clients"][case]["result"]["accepted"],
                "TLS case {case}: Go={oracle}, Rust={rust}"
            );
            if rust["clients"][case]["result"]["accepted"] == true {
                assert_eq!(rust["clients"][case]["result"]["health_status"], 1);
                assert_eq!(
                    rust["clients"][case]["result"],
                    oracle["clients"][case]["result"]
                );
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_tls_flags_merge_nonempty_values_over_files() {
    let certs = certificates("mcs-flags");
    let good = certs.0.join("good.toml");
    let bad = certs.0.join("bad.toml");
    let ca = certs.0.join("ca.pem").display().to_string();
    let cert = certs.0.join("server.pem").display().to_string();
    let key = certs.0.join("server.key").display().to_string();
    let text = |cert: &str| {
        format!(
            "[security]\ncacert-path={}\ncert-path={}\nkey-path={}\n",
            json!(ca),
            json!(cert),
            json!(key)
        )
    };
    std::fs::write(&good, text(&cert)).unwrap();
    std::fs::write(&bad, text("missing-file-certificate")).unwrap();
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        for (case, args) in [
            (
                "empty certificate flag",
                vec![format!("--config={}", good.display()), "--cert=".into()],
            ),
            (
                "empty configuration flag",
                vec![
                    "--config=".into(),
                    format!("--cacert={ca}"),
                    format!("--cert={cert}"),
                    format!("--key={key}"),
                ],
            ),
            (
                "nonempty certificate override",
                vec![
                    format!("--config={}", bad.display()),
                    format!("--cert={cert}"),
                ],
            ),
        ] {
            let args = args.iter().map(String::as_str).collect::<Vec<_>>();
            let mut sidecar = Sidecar::start_service_with_args(
                binary,
                pd.metadata_url(),
                "resource-manager",
                "http",
                &args,
            );
            let result = ready(&mut sidecar, &certs.0).await;
            if result["accepted"] == true {
                sidecar.stop();
            }
            records.push(json!({"rust":is_rust,"case":case,"ready":result}));
        }
        pd.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("mcs-flags.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    assert_eq!(records.len(), 6);
    for row in &records[..3] {
        assert_eq!(row["ready"]["accepted"], true, "Go startup: {row}");
    }
    for n in 0..3 {
        assert_eq!(
            records[n + 3]["ready"],
            records[n]["ready"],
            "flag merge case: {}",
            records[n + 3]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_tls_without_ca_verifies_supplied_clients_against_system_roots() {
    let certs = certificates("mcs-system-roots");
    let cfg = certs.0.join("no-ca.toml");
    std::fs::write(
        &cfg,
        format!(
            "[security]\ncert-path={}\nkey-path={}\n",
            json!(certs.0.join("server.pem")),
            json!(certs.0.join("server.key"))
        ),
    )
    .unwrap();
    let arg = format!("--config={}", cfg.display());
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        let mut sidecar = Sidecar::start_service_with_args(
            binary,
            pd.metadata_url(),
            "resource-manager",
            "http",
            &[&arg],
        );
        let ready = ready_as(&mut sidecar, &certs.0, None).await;
        let mut clients = vec![];
        if ready["accepted"] == true {
            for identity in [
                None,
                Some("allowed"),
                Some("denied"),
                Some("rogue"),
                Some("server"),
            ] {
                clients.push(json!({"identity":identity,"result":probe(&sidecar.url,&certs.0,identity).await}));
            }
            sidecar.stop();
        }
        records.push(json!({"rust":is_rust,"ready":ready,"clients":clients}));
        pd.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("mcs-system-roots.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for row in &records {
        assert_eq!(row["ready"]["accepted"], true, "{row}");
        for (case, expected) in [true, false, false, false, false].into_iter().enumerate() {
            assert_eq!(
                row["clients"][case]["result"]["accepted"], expected,
                "case {case}: {row}"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_controller_toml_keeps_go_boolean_and_numeric_forms() {
    let certs = certificates("mcs-controller-toml");
    let cfg = certs.0.join("controller.toml");
    std::fs::write(&cfg,format!("[security]\ncacert-path={}\ncert-path={}\nkey-path={}\n[controller]\nenable-controller-trace-log=true\n[controller.request-unit]\nread-base-cost=0.0\nwrite-base-cost=2\n",json!(certs.0.join("ca.pem")),json!(certs.0.join("server.pem")),json!(certs.0.join("server.key")))).unwrap();
    let arg = format!("--config={}", cfg.display());
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        let mut sidecar = Sidecar::start_service_with_args(
            binary,
            pd.metadata_url(),
            "resource-manager",
            "http",
            &[&arg],
        );
        let result = ready(&mut sidecar, &certs.0).await;
        if result["accepted"] == true {
            sidecar.stop();
        }
        records.push(json!({"rust":is_rust,"ready":result}));
        pd.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("mcs-controller-toml.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    assert_eq!(records[0]["ready"]["accepted"], true, "Go: {}", records[0]);
    assert_eq!(
        records[1]["ready"], records[0]["ready"],
        "Rust: {}",
        records[1]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_016_mcs_invalid_raw_client_credentials_fail_before_serving() {
    let work = super::accept_extra_transport_config_test::Work(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("mcs-invalid-raw-{}", std::process::id())),
    );
    std::fs::create_dir_all(&work.0).unwrap();
    let cfg = work.0.join("raw.toml");
    std::fs::write(&cfg, "[security]\nSSLCABytes=[1]\n").unwrap();
    let arg = format!("--config={}", cfg.display());
    let go = crate::readonly_oracle::pinned_go_binary().unwrap();
    let rust = PathBuf::from(std::env::var_os("PD_COMPAT_RUST_BIN").unwrap());
    let mut records = vec![];
    for (binary, is_rust) in [(&go, false), (&rust, true)] {
        let mut pd = Server::start(&go).unwrap();
        pd.ready().unwrap();
        let mut sidecar = Sidecar::start_service_with_args(
            binary,
            pd.metadata_url(),
            "resource-manager",
            "http",
            &[&arg],
        );
        let deadline = Instant::now() + Duration::from_secs(5);
        let result = loop {
            if let Some(status) = sidecar.child.try_wait().unwrap() {
                break json!({"exited":true,"success":status.success(),"logs":sidecar.logs()});
            }
            if let Ok(mut client) = HealthClient::connect(sidecar.url.clone()).await {
                if client.check(HealthCheckRequest::default()).await.is_ok() {
                    break json!({"exited":false,"serving":true});
                }
            }
            if Instant::now() >= deadline {
                break json!({"exited":false,"serving":false,"timeout":true});
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        };
        if result["exited"] != true {
            sidecar.stop();
        }
        records.push(json!({"rust":is_rust,"result":result}));
        pd.stop().unwrap();
    }
    let path = PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("mcs-invalid-raw.json");
    std::fs::write(path, serde_json::to_vec(&records).unwrap()).unwrap();
    for row in &records {
        assert_eq!(row["result"]["exited"], true, "{row}");
        assert_eq!(row["result"]["success"], false, "{row}");
    }
}
