// Copyright 2026 AsterSQL.
use astersql_pd_compat_tests::{
    harness::{self, Server},
    normalize::{Rules, differences},
};
use serde_json::json;
#[test]
fn detects_semantic_difference() {
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_pd-compat-different-service"));
    let mut go = Server::start(binary).unwrap();
    let mut rust = Server::start_with_args(binary, &["--semantic-difference"]).unwrap();
    go.ready().unwrap();
    rust.ready().unwrap();
    let diff = differences(&go.sample().unwrap(), &rust.sample().unwrap());
    assert_eq!(diff, vec!["$/config/schedule.max-snapshot-count: 64 != 65"]);
    println!("{}", diff.join("\n"));
    let dirs = [go.dir.clone(), rust.dir.clone()];
    drop((go, rust));
    assert!(dirs.iter().all(|d| !d.exists()));
}
#[test]
fn normalization_preserves_semantics() {
    let rules = Rules(vec![(json!(42), json!("<member>"))]);
    assert_eq!(
        rules.apply(&json!({"id":42,"health":false})),
        json!({"id":"<member>","health":false})
    );
    assert!(!differences(&json!([1, 2]), &json!([2, 1])).is_empty());
    assert!(differences(&json!({"a":1,"b":2}), &json!({"b":2,"a":1})).is_empty());
}
#[test]
fn health_member_processes() {
    let Ok(mode) = std::env::var("PD_COMPAT_MODE") else {
        // Exercise actual process lifecycle even without an installed Go oracle.
        let binary = std::path::Path::new(env!("CARGO_BIN_EXE_pd-compat-different-service"));
        harness::run("dual", binary, Some(binary)).unwrap();
        return;
    };
    let go = std::env::var_os("PD_COMPAT_GO_BIN").unwrap_or_default();
    let rust = std::env::var_os("PD_COMPAT_RUST_BIN");
    harness::run(
        &mode,
        std::path::Path::new(&go),
        rust.as_deref().map(std::path::Path::new),
    )
    .unwrap();
}

#[test]
fn normalization_does_not_hide_equal_valued_config() {
    let rules = Rules(vec![(json!(42), json!("<member>"))]);
    assert_eq!(
        rules.apply(&json!({"member_id":42,"max-replicas":42}))["max-replicas"],
        42
    );
}

#[test]
fn protobuf_clock_offset_preserves_logical_difference() {
    use astersql_pd_compat_tests::normalize::tso;
    use astersql_pd_proto::pdpb::{Timestamp, TsoResponse};
    use prost::Message;
    let a = TsoResponse {
        count: 1,
        timestamp: Some(Timestamp {
            physical: 100,
            logical: 2,
            suffix_bits: 0,
        }),
        ..Default::default()
    };
    let mut b = a.clone();
    b.timestamp.as_mut().unwrap().physical = 200;
    assert_eq!(
        tso(&a.encode_to_vec(), 100).unwrap(),
        tso(&b.encode_to_vec(), 200).unwrap()
    );
    b.timestamp.as_mut().unwrap().logical = 3;
    assert_eq!(
        differences(
            &tso(&a.encode_to_vec(), 100).unwrap(),
            &tso(&b.encode_to_vec(), 200).unwrap()
        ),
        vec!["$/timestamp/logical: 2 != 3"]
    );
    assert!(tso(&[255], 0).is_err());
}

#[test]
fn metric_values_and_types_are_semantic() {
    use astersql_pd_compat_tests::normalize::metrics;
    assert_eq!(
        metrics("# TYPE pd_test gauge\npd_test 1\n"),
        metrics("pd_test 1\n# TYPE pd_test gauge\n")
    );
    assert_ne!(metrics("pd_test 1"), metrics("pd_test 2"));
    assert_ne!(
        metrics("# TYPE pd_test gauge"),
        metrics("# TYPE pd_test counter")
    );
}

#[test]
fn failed_start_cleans_directory() {
    assert!(Server::start(std::path::Path::new("/nonexistent/pd-compat-server")).is_err());
}

#[test]
fn early_exit_is_reported_and_cleaned() {
    let mut server = Server::start(std::path::Path::new("/usr/bin/false")).unwrap();
    let dir = server.dir.clone();
    assert!(server.ready().unwrap_err().contains("server exited"));
    drop(server);
    assert!(!dir.exists());
}

#[test]
fn readiness_timeout_cleans_process_and_data() {
    let binary = std::path::Path::new(env!("CARGO_BIN_EXE_pd-compat-different-service"));
    let mut server = Server::start_with_args(binary, &["--unhealthy"]).unwrap();
    let dir = server.dir.clone();
    let url = server.url.clone();
    assert!(
        server
            .ready_with_timeout(std::time::Duration::from_millis(150))
            .unwrap_err()
            .contains("readiness timeout")
    );
    drop(server);
    assert!(!dir.exists());
    assert!(std::net::TcpStream::connect(url.trim_start_matches("http://")).is_err());
}
