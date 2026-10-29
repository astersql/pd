// Copyright 2026 AsterSQL.
#[allow(dead_code)]
#[path = "../../pkg/storage/kv/etcd_fixture_test.rs"]
mod fixture;
#[test]
fn standalone_runtime_lifecycle() {
    let fixture = fixture::Fixture::start();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let events = runtime
        .block_on(crate::entry::run_until(
            crate::entry::Config {
                diagnostic_endpoint: None,
                diagnostic_member: None,
                endpoint: fixture.endpoint.clone(),
                client: "http://127.0.0.1:0".into(),
                peer: "http://127.0.0.1:2380".into(),
                name: "standalone".into(),
                advertise_client: None,
                advertise_peer: None,
            },
            async { Ok(()) },
        ))
        .unwrap();
    assert_eq!(
        events,
        vec![
            "storage",
            "leadership",
            "services",
            "listener",
            "ready",
            "stop-admission",
            "background-joined",
            "leadership-released",
            "storage-released"
        ]
    );
    println!("standalone: {events:?}");
}

#[test]
fn task_039_cli_advertise_options() {
    let config = crate::entry::Config::parse(
        [
            "--etcd-endpoints=http://127.0.0.1:2379",
            "--client-urls=http://0.0.0.0:0",
            "--advertise-client-urls=http://public.example:2379",
            "--advertise-peer-urls=http://public.example:2380",
        ]
        .into_iter()
        .map(String::from),
    )
    .unwrap();
    assert_eq!(config.client, "http://0.0.0.0:0");
    assert_eq!(
        config.advertise_client.as_deref(),
        Some("http://public.example:2379")
    );
    assert_eq!(
        config.advertise_peer.as_deref(),
        Some("http://public.example:2380")
    );
}

#[test]
fn task_043_explicit_member_flags() {
    let parse = |extra: &[&str]| {
        crate::entry::Config::parse(
            std::iter::once("--etcd-endpoints=http://127.0.0.1:2379".to_owned())
                .chain(extra.iter().map(|s| s.to_string())),
        )
    };
    assert!(parse(&["--etcd-member-endpoint=http://127.0.0.1:2379"]).is_err());
    assert!(parse(&["--etcd-member-id=1"]).is_err());
    assert!(
        parse(&[
            "--etcd-member-endpoint=http://127.0.0.1:2379",
            "--etcd-member-id=0"
        ])
        .is_err()
    );
    let config = parse(&[
        "--etcd-member-endpoint=http://127.0.0.1:2379",
        "--etcd-member-id=42",
    ])
    .unwrap();
    assert_eq!(config.diagnostic_member, Some(42));
    assert_eq!(
        config.diagnostic_endpoint.as_deref(),
        Some("http://127.0.0.1:2379")
    );
}
