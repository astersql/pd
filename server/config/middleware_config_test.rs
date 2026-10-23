// Copyright 2026 AsterSQL.
use super::*;
fn dimension() -> MiddlewareDimensionConfig {
    MiddlewareDimensionConfig {
        qps: 1.5,
        qps_burst: -2,
        concurrency_limit: u64::MAX,
    }
}
#[test]
fn middleware_config_constructor_go_bytes() {
    let cfg = ServiceMiddlewareConfig::new();
    let expected = include_str!("../../docs/rust/theme3-服务配置契约.md")
        .split("<!-- batch95-go-middleware -->")
        .nth(1)
        .unwrap()
        .split("```json\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&cfg).unwrap()),
        expected
    );
    assert!(cfg.audit_config.enable_audit);
    assert!(cfg.rate_limit_config.enable_rate_limit);
    assert!(cfg.grpc_rate_limit_config.enable_rate_limit);
}
#[test]
fn middleware_config_zero_and_nil_clone() {
    let zero = ServiceMiddlewareConfig::default();
    let value = serde_json::to_value(&zero).unwrap();
    assert_eq!(value["audit"]["enable-audit"], "false");
    assert!(value["rate-limit"]["limiter-config"].is_null());
    assert!(value["grpc-rate-limit"]["grpc-limiter-config"].is_null());
    assert!(zero.clone().rate_limit_config.limiter_config.is_none());
    assert!(zero.rate_limit_config.clone().limiter_config.is_some());
    assert!(zero.grpc_rate_limit_config.clone().limiter_config.is_some());
}
#[test]
fn middleware_config_clone_map_ownership() {
    let cfg = ServiceMiddlewareConfig::new();
    let top = cfg.clone();
    for map in [
        &top.rate_limit_config.limiter_config,
        &top.grpc_rate_limit_config.limiter_config,
    ] {
        map.as_ref().unwrap().insert("test".into(), dimension());
    }
    assert_eq!(
        cfg.rate_limit_config
            .limiter_config
            .as_ref()
            .unwrap()
            .get("test"),
        Some(dimension())
    );
    assert_eq!(
        cfg.grpc_rate_limit_config
            .limiter_config
            .as_ref()
            .unwrap()
            .get("test"),
        Some(dimension())
    );
    let http = cfg.rate_limit_config.clone();
    let grpc = cfg.grpc_rate_limit_config.clone();
    http.limiter_config
        .unwrap()
        .insert("test".into(), MiddlewareDimensionConfig::default());
    grpc.limiter_config
        .unwrap()
        .insert("test".into(), MiddlewareDimensionConfig::default());
    assert_eq!(
        cfg.rate_limit_config.limiter_config.unwrap().get("test"),
        Some(dimension())
    );
    assert_eq!(
        cfg.grpc_rate_limit_config
            .limiter_config
            .unwrap()
            .get("test"),
        Some(dimension())
    );
    let mut audit = cfg.audit_config.clone();
    audit.enable_audit = false;
    assert!(!audit.enable_audit);
    assert!(cfg.audit_config.enable_audit);
}
#[test]
fn middleware_config_dimension_and_decode() {
    let encoded = serde_json::to_string(&dimension()).unwrap();
    assert_eq!(
        encoded,
        r#"{"QPS":1.5,"QPSBurst":-2,"ConcurrencyLimit":18446744073709551615}"#
    );
    assert_eq!(
        serde_json::from_str::<MiddlewareDimensionConfig>(&encoded).unwrap(),
        dimension()
    );
    for input in [
        r#"{"audit":{"enable-audit":true}}"#,
        r#"{"rate-limit":{"limiter-config":[]}}"#,
        r#"{"grpc-rate-limit":{"enable-grpc-rate-limit":"yes"}}"#,
    ] {
        assert!(
            serde_json::from_str::<ServiceMiddlewareConfig>(input).is_err(),
            "{input}"
        );
    }
    let cfg: ServiceMiddlewareConfig = serde_json::from_str(
        r#"{"audit":{"enable-audit":null},"rate-limit":{"enable-rate-limit":"null"}}"#,
    )
    .unwrap();
    assert!(!cfg.audit_config.enable_audit);
    assert!(!cfg.rate_limit_config.enable_rate_limit);
    assert!(
        serde_json::from_str::<MiddlewareDimensionConfig>(r#"{"ConcurrencyLimit":-1}"#).is_err()
    );
}

#[test]
fn middleware_config_populated_go_bytes() {
    let expected = include_str!("../../docs/rust/theme3-服务配置契约.md")
        .split("<!-- batch95-go-populated -->")
        .nth(1)
        .unwrap()
        .split("```json\n")
        .nth(1)
        .unwrap()
        .split("```")
        .next()
        .unwrap();
    let cfg = ServiceMiddlewareConfig::new();
    cfg.rate_limit_config
        .limiter_config
        .as_ref()
        .unwrap()
        .insert(
            "GetHealthStatus".into(),
            MiddlewareDimensionConfig {
                qps: 2.5,
                qps_burst: 2,
                concurrency_limit: 200,
            },
        );
    cfg.grpc_rate_limit_config
        .limiter_config
        .as_ref()
        .unwrap()
        .insert(
            "GetMembers".into(),
            MiddlewareDimensionConfig {
                qps: 0.5,
                qps_burst: 1,
                concurrency_limit: 300,
            },
        );
    assert_eq!(
        format!("{}\n", serde_json::to_string_pretty(&cfg).unwrap()),
        expected
    );
    let decoded: ServiceMiddlewareConfig = serde_json::from_str(expected).unwrap();
    assert_eq!(
        decoded
            .rate_limit_config
            .limiter_config
            .as_ref()
            .unwrap()
            .get("GetHealthStatus"),
        cfg.rate_limit_config
            .limiter_config
            .as_ref()
            .unwrap()
            .get("GetHealthStatus")
    );
    assert_eq!(
        decoded
            .grpc_rate_limit_config
            .limiter_config
            .as_ref()
            .unwrap()
            .get("GetMembers"),
        cfg.grpc_rate_limit_config
            .limiter_config
            .as_ref()
            .unwrap()
            .get("GetMembers")
    );
}

#[test]
fn middleware_config_qps_number_encoding() {
    // Go encoding/json::floatEncoder selects fixed notation in [1e-6, 1e21).
    for (qps, expected) in [
        (0.0, "0"),
        (-0.0, "-0"),
        (2.0, "2"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (1e21, "1e+21"),
    ] {
        let value = MiddlewareDimensionConfig {
            qps,
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&value).unwrap(),
            format!("{{\"QPS\":{expected},\"QPSBurst\":0,\"ConcurrencyLimit\":0}}")
        );
    }
    for qps in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(
            serde_json::to_string(&MiddlewareDimensionConfig {
                qps,
                ..Default::default()
            })
            .is_err()
        );
    }
}
