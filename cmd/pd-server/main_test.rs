// Copyright 2026 AsterSQL.
#[allow(dead_code)]
#[path = "../../pkg/storage/etcd_fixture_test.rs"]
mod fixture;
#[test]
fn standalone_runtime_lifecycle() {
    let fixture = fixture::Fixture::start();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    drop(listener);
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    let events = runtime
        .block_on(crate::entry::run_until(
            crate::entry::Config {
                endpoint: fixture.endpoint.clone(),
                client: format!("http://{address}"),
                peer: "http://127.0.0.1:2380".into(),
                name: "standalone".into(),
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
    assert!(std::net::TcpStream::connect(address).is_err());
    println!("standalone: {events:?}");
}
