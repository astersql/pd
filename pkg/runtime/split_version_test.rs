// Copyright 2026 AsterSQL.
use super::compare_version;
use astersql_pd_server_config::ClusterVersion;
fn version(s: &str) -> ClusterVersion {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap()
}
#[test]
fn task_026_batch_split_version_boundary() {
    // Go versioninfo's BatchSplit threshold is 2.1.0-rc.1. Numeric release
    // identifiers compare numerically; metadata does not affect precedence.
    let threshold = version("2.1.0-rc.1");
    for text in ["0.0.0", "2.0.99", "2.1.0-beta.999", "2.1.0-rc.0"] {
        assert!(
            compare_version(&version(text), &threshold).is_lt(),
            "{text}"
        );
    }
    for text in [
        "2.1.0-rc.1",
        "2.1.0-rc.1+build",
        "2.1.0-rc.2",
        "2.1.0-rc.10",
        "2.1.0",
        "2.1.1-alpha",
        "8.5.0",
    ] {
        assert!(
            !compare_version(&version(text), &threshold).is_lt(),
            "{text}"
        );
    }
    assert!(compare_version(&version("2.1.0-rc.2"), &version("2.1.0-rc.10")).is_lt());
}
