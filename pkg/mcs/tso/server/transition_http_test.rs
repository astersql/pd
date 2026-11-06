// Copyright 2026 AsterSQL.
use astersql_pd_storage_domain::{KeyspaceGroup, MergeState, SplitState};
use std::{path::PathBuf, process::Command, sync::Arc};
use tokio_util::sync::CancellationToken;
struct Certificates(PathBuf);
impl Drop for Certificates {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tso_transition_https_authenticates_and_sends_exact_completion_request() {
    let certs = Certificates(
        PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
            .join(format!("tso-transition-tls-{}", std::process::id())),
    );
    assert!(
        Command::new("python3")
            .args([
                "../../../../scripts/service-auth-oracle.py",
                "--certificates"
            ])
            .arg(&certs.0)
            .status()
            .unwrap()
            .success()
    );
    let mut config = astersql_pd_server_config::Config::default();
    config.security.tls.ca_path = certs.0.join("ca.pem").display().to_string();
    config.security.tls.cert_path = certs.0.join("server.pem").display().to_string();
    config.security.tls.key_path = certs.0.join("server.key").display().to_string();
    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("https://{}", tcp.local_addr().unwrap());
    let listener = astersql_pd_server::service_auth::Listener::new(tcp, &config).unwrap();
    let (sent, mut observed) = tokio::sync::mpsc::unbounded_channel();
    let router = axum::Router::new().fallback(move |request: axum::extract::Request| {
        let sent = sent.clone();
        async move {
            sent.send((request.method().clone(), request.uri().to_string()))
                .unwrap();
            if request.uri().path().contains("/4/") {
                axum::http::StatusCode::SERVICE_UNAVAILABLE
            } else {
                axum::http::StatusCode::OK
            }
        }
    });
    let cancel = CancellationToken::new();
    let stopped = cancel.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, router)
            .with_graceful_shutdown(stopped.cancelled_owned())
            .await
            .unwrap()
    });
    let group = KeyspaceGroup {
        id: 2,
        split_state: Some(SplitState { split_source: 1 }),
        ..Default::default()
    };
    let security = config.security.tls.clone();
    let policy: astersql_pd_mcs_server::ClientTlsFactory =
        Arc::new(move || astersql_pd_server::client_auth::configuration(&security).map(Arc::new));
    let result = super::transition_http::finish(&url, &group, &cancel, Some(&policy)).await;
    assert!(result.is_ok(), "HTTPS transition failed: {result:?}");
    let merge = KeyspaceGroup {
        id: 3,
        merge_state: Some(MergeState {
            merge_list: Some(vec![1]),
        }),
        ..Default::default()
    };
    assert!(
        super::transition_http::finish(&url, &merge, &cancel, Some(&policy))
            .await
            .is_ok()
    );
    let failed = KeyspaceGroup {
        id: 4,
        ..merge.clone()
    };
    assert!(matches!(
        super::transition_http::finish(&url, &failed, &cancel, Some(&policy)).await,
        Err(astersql_pd_tso::TsoError::WindowLost)
    ));
    // Missing policy and authenticated-but-untrusted/anonymous clients must not
    // reach the application or fall back to cleartext.
    assert!(
        super::transition_http::finish(&url, &group, &cancel, None)
            .await
            .is_err()
    );
    let mut anonymous = config.security.tls.clone();
    anonymous.cert_path.clear();
    anonymous.key_path.clear();
    let anonymous: astersql_pd_mcs_server::ClientTlsFactory =
        Arc::new(move || astersql_pd_server::client_auth::configuration(&anonymous).map(Arc::new));
    assert!(
        super::transition_http::finish(&url, &group, &cancel, Some(&anonymous))
            .await
            .is_err()
    );
    let mut untrusted = config.security.tls.clone();
    untrusted.ca_path = certs.0.join("rogue.pem").display().to_string();
    let untrusted: astersql_pd_mcs_server::ClientTlsFactory =
        Arc::new(move || astersql_pd_server::client_auth::configuration(&untrusted).map(Arc::new));
    assert!(
        super::transition_http::finish(&url, &group, &cancel, Some(&untrusted))
            .await
            .is_err()
    );
    let interrupted = CancellationToken::new();
    interrupted.cancel();
    assert!(matches!(
        super::transition_http::finish(&url, &group, &interrupted, Some(&policy)).await,
        Err(astersql_pd_tso::TsoError::Cancelled)
    ));
    cancel.cancel();
    server.await.unwrap();
    for (id, operation) in [(2, "split"), (3, "merge"), (4, "merge")] {
        assert_eq!(
            observed.try_recv().unwrap(),
            (
                axum::http::Method::DELETE,
                format!("/pd/api/v2/tso/keyspace-groups/{id}/{operation}")
            )
        );
    }
    assert!(observed.try_recv().is_err());
    assert_eq!(tokio::spawn(async { 17 }).await.unwrap(), 17);
}
