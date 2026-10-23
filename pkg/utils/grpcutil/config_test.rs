// Copyright 2026 AsterSQL.
use super::*;
#[test]
fn tls_bytes_null_empty_and_base64() {
    let c: TLSConfig = serde_json::from_str(
        r#"{"SSLCABytes":"YWJj","SSLCertBytes":[],"SSLKEYBytes":null,"cert-allowed-cn":[]}"#,
    )
    .unwrap();
    assert_eq!(c.sslca_bytes, Some(b"abc".to_vec()));
    assert_eq!(c.ssl_cert_bytes, Some(vec![]));
    assert_eq!(c.sslkey_bytes, None);
    assert_eq!(c.cert_allowed_c_ns, Some(vec![]));
    let v = serde_json::to_value(c).unwrap();
    assert_eq!(v["SSLCABytes"], "YWJj");
    assert_eq!(v["SSLCertBytes"], "");
    assert!(v["SSLKEYBytes"].is_null());
    assert!(serde_json::from_str::<TLSConfig>(r#"{"SSLCABytes":"!"}"#).is_err());
}
