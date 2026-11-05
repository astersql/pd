// Copyright 2026 AsterSQL.
use crate::{meta_group_http::check_endpoint, tls_listener_test::Certificates};
use std::time::Duration;
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sa_012_fresh_tls_health() {
    use axum::serve::Listener as _;
    let fixture = crate::node_test::fixture::Fixture::start();
    let certs = Certificates::new();
    let config = certs.config();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("https://{}", tcp.local_addr().unwrap());
    let mut listener = astersql_pd_server::service_auth::Listener::new(tcp, &config).unwrap();
    let cancel = tokio_util::sync::CancellationToken::new();
    let c = cancel.clone();
    let upstream = fixture.endpoint.trim_start_matches("http://").to_owned();
    let proxy = tokio::spawn(async move {
        let mut sessions = tokio::task::JoinSet::new();
        loop {
            tokio::select! {
                _=c.cancelled()=>break,
                (mut incoming,_) = listener.accept()=>{let upstream=upstream.clone();sessions.spawn(async move {let mut outgoing=tokio::net::TcpStream::connect(upstream).await.unwrap();let _=tokio::io::copy_bidirectional(&mut incoming,&mut outgoing).await;});}
            }
        }
    });
    check_endpoint(&config, &endpoint).await.unwrap();
    // Every call rereads the current input files instead of keeping a cached channel.
    let mut missing = config.clone();
    missing.security.tls.ca_path = certs.path("missing-ca.pem");
    assert!(
        check_endpoint(&missing, &endpoint)
            .await
            .unwrap_err()
            .starts_with("failed to load TLS config:")
    );
    let mut rogue = config.clone();
    rogue.security.tls.cert_path = certs.path("rogue.pem");
    rogue.security.tls.key_path = certs.path("rogue.key");
    assert!(check_endpoint(&rogue, &endpoint).await.is_err());
    check_endpoint(&config, &endpoint).await.unwrap();
    cancel.cancel();
    tokio::time::timeout(Duration::from_secs(3), proxy)
        .await
        .unwrap()
        .unwrap();
}
