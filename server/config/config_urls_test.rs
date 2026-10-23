// Copyright 2026 AsterSQL.
use crate::{ConfigScalars, validate_url_list, validate_url_with_scheme};

#[test]
fn config_urls_scheme_matches_go_table() {
    // server/config/util_test.go::TestValidateURLWithScheme, in source order.
    for (raw, invalid) in [
        ("", true),
        ("foo", true),
        ("/foo", true),
        ("http", true),
        ("http://", true),
        ("http://foo", false),
        ("https://foo", false),
        ("http://127.0.0.1", false),
        ("http://127.0.0.1/", false),
        ("http://[::1]:2379", false),
        ("http://::1:2379", true),
        ("http://localhost:80:80", true),
        ("https://foo.com/bar", false),
        ("https://foo.com/bar/", false),
    ] {
        assert_eq!(validate_url_with_scheme(raw).is_err(), invalid, "{raw:?}");
    }
    assert_eq!(
        validate_url_with_scheme("/foo").unwrap_err().to_string(),
        "/foo has no scheme"
    );
    assert_eq!(
        validate_url_with_scheme("http://").unwrap_err().to_string(),
        "http:// has no scheme"
    );
}

#[test]
fn config_urls_join_matches_go_cases() {
    // config_test.go::TestBadFormatJoinAddr / TestJoinAddr.
    for (join, invalid) in [
        ("127.0.0.1:2379", true),
        ("http://127.0.0.1:2379", false),
        ("http://127.0.0.1:2379,http://127.0.0.1:2381", false),
        (
            "http://pd-0.pd-peer:2379,https://pd-1.pd-peer:2379,http://[::1]:2379",
            false,
        ),
        (
            "http://demo-pd-0.demo-pd-peer.demo.svc:2380,http://demo-pd-1.demo-pd-peer.demo.svc:2380",
            false,
        ),
        ("http://127.0.0.1:2379,127.0.0.1:2381", true),
    ] {
        let config = ConfigScalars {
            join: join.into(),
            ..Default::default()
        };
        assert_eq!(config.validate_join().is_err(), invalid, "{join:?}");
    }
}

#[test]
fn config_urls_list_preserves_parse_not_request_semantics() {
    // Derived from Go util.go::parseUrls and Go 1.26 net/url::parse.
    for raw in [
        "",
        ",",
        "foo",
        "/foo",
        "http://",
        "mailto:pd@example.com",
        "//pd:2379",
        "http://pd:2379,",
        "http://pd:2379,,/relative",
        "custom://a:1:2",
        "http://pd:99999",
        "http://pd/path?q=%zz",
        "http://pd/path#fragment",
    ] {
        assert!(validate_url_list(raw).is_ok(), "{raw:?}");
    }
    for raw in [
        "http://pd:2379,127.0.0.1:2381",
        "http://pd:2379,http://bad:port",
        "http://pd/%zz",
        "http://[::1",
        "http://[127.0.0.1]",
        "http://[bad]",
        "http://a b",
        "http://%41",
        "http://pd/#%zz",
        "http://pd/\n",
        "http://pd:2379, https://pd:2379",
    ] {
        assert!(validate_url_list(raw).is_err(), "{raw:?}");
    }
}

#[test]
fn config_urls_request_and_join_boundaries() {
    for raw in [
        "custom://pd",
        "HTTP://pd",
        "http://pd:99999",
        "http://u:p@pd",
        "http://[fe80::1%25en0]:2379",
        "http://pd/%20",
        "http://pd/path?q=%zz",
        "http://a<b>\"",
    ] {
        assert!(validate_url_with_scheme(raw).is_ok(), "{raw:?}");
    }
    for raw in [
        "//pd:2379",
        "mailto:pd@example.com",
        "http:/pd",
        "http://pd/%zz",
        "http://[::1]junk",
        "http://u|p@pd",
        "http://pd/path#%zz",
    ] {
        assert!(validate_url_with_scheme(raw).is_err(), "{raw:?}");
    }
    let mut config = ConfigScalars::default();
    assert!(config.validate_join().is_ok());
    config.join = "bad:opaque".into();
    assert!(config.validate_join().is_ok());
    config.initial_cluster = "pd=http://pd:2380".into();
    assert_eq!(
        config.validate_join().unwrap_err().to_string(),
        "-initial-cluster and -join can not be provided at the same time"
    );
    config.initial_cluster.clear();
    config.join = "127.0.0.1:2379".into();
    let error = config.validate_join().unwrap_err().to_string();
    assert!(
        error.starts_with("failed to parse join addr:127.0.0.1:2379, err:"),
        "{error}"
    );
    assert!(error.contains("PD:url:ErrURLParse"), "{error}");
    assert!(
        error.contains("first path segment in URL cannot contain colon"),
        "{error}"
    );
}
