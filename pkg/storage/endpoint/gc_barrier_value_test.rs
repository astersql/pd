// Copyright 2026 AsterSQL.

use super::*;

// Captured from the existing Go PD binary, /tmp/pd-gc28-go-wire.json.
// Exact wire bytes are independent of the Rust encoder; see theme3 evidence.
const GO_WIRE: &[(bool, u32, &str, i64, &str)] = &[
    (
        true,
        4294967295,
        r#"屏障 <>&   end3600"#,
        1792742540,
        r#"{"barrier_id":"屏障 \u003c\u003e\u0026 \u2028 end3600","barrier_ts":18446744073709551614,"expiration_time":1792742540}"#,
    ),
    (
        true,
        4294967295,
        r#"屏障 <>&   end9223372036854775807"#,
        0,
        r#"{"barrier_id":"屏障 \u003c\u003e\u0026 \u2028 end9223372036854775807","barrier_ts":18446744073709551614,"expiration_time":0}"#,
    ),
    (
        false,
        4294967295,
        r#"屏障 <>&   end03600"#,
        1792742540,
        r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end03600","expired_at":1792742540,"safe_point":18446744073709551614}"#,
    ),
    (
        false,
        4294967295,
        r#"屏障 <>&   end09223372036854775807"#,
        9223372036854775807,
        r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end09223372036854775807","expired_at":9223372036854775807,"safe_point":18446744073709551614}"#,
    ),
    (
        false,
        4294967295,
        r#"屏障 <>&   endNone3600"#,
        1792742540,
        r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 endNone3600","expired_at":1792742540,"safe_point":18446744073709551614}"#,
    ),
    (
        false,
        4294967295,
        r#"屏障 <>&   endNone9223372036854775807"#,
        9223372036854775807,
        r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 endNone9223372036854775807","expired_at":9223372036854775807,"safe_point":18446744073709551614}"#,
    ),
    (
        false,
        1,
        r#"屏障 <>&   end13600"#,
        1792742540,
        r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end13600","expired_at":1792742540,"safe_point":18446744073709551614,"keyspace_id":1}"#,
    ),
    (
        false,
        1,
        r#"屏障 <>&   end19223372036854775807"#,
        9223372036854775807,
        r#"{"service_id":"屏障 \u003c\u003e\u0026 \u2028 end19223372036854775807","expired_at":9223372036854775807,"safe_point":18446744073709551614,"keyspace_id":1}"#,
    ),
];

#[test]
fn gc_barrier_value_go_bytes() {
    for &(global, keyspace, id, seconds, wire) in GO_WIRE {
        let finite = if global {
            seconds != 0
        } else {
            seconds != i64::MAX
        };
        let mut time = BarrierTime::from_unix(if finite { seconds - 1 } else { 42 }, 1);
        let expiration = finite.then_some(&mut time);
        let actual = if global {
            let barrier = GlobalGCBarrier::new(id, u64::MAX - 1, expiration);
            let bytes = barrier.to_json().unwrap();
            assert_eq!(
                GlobalGCBarrier::from_json(wire.as_bytes()).unwrap(),
                barrier
            );
            bytes
        } else {
            let barrier = GCBarrier::new(id, u64::MAX - 1, expiration);
            let ssp = barrier.to_service_safe_point(keyspace);
            assert_eq!(ServiceSafePoint::from_json(wire.as_bytes()).unwrap(), ssp);
            assert_eq!(GCBarrier::from_service_safe_point(&ssp), barrier);
            ssp.to_json().unwrap()
        };
        assert_eq!(actual, wire.as_bytes(), "Go wire for {id}");
    }
}

#[test]
fn gc_barrier_value_rounding_and_input_mutation() {
    for seconds in [-2, -1, 0, 1, 1_740_000_000, i64::MAX - 1] {
        for nanos in [0, 1, 1_000_000, 999_000_000, 999_999_999] {
            let rounded = BarrierTime::from_unix(seconds + i64::from(nanos != 0), 0);
            let mut input = BarrierTime::from_unix(seconds, nanos);
            let barrier = GCBarrier::new("x", 42, Some(&mut input));
            assert_eq!(input, rounded, "GC constructor rounds and mutates input");
            assert_eq!(barrier.expiration_time, Some(rounded));
            let mut input = BarrierTime::from_unix(seconds, nanos);
            let barrier = GlobalGCBarrier::new("x", 42, Some(&mut input));
            assert_eq!(
                input, rounded,
                "global constructor rounds and mutates input"
            );
            assert_eq!(barrier.expiration_time, Some(rounded));
        }
    }
    assert_eq!(BarrierTime::from_unix(0, -1).unix_seconds(), -1);
    assert_eq!(
        BarrierTime::from_unix(-1, 1_000_000_001),
        BarrierTime::from_unix(0, 1)
    );
}

#[test]
fn gc_barrier_value_strict_expiration() {
    let end = BarrierTime::from_unix(1234, 0);
    let before = BarrierTime::from_unix(1233, 999_999_999);
    let after = BarrierTime::from_unix(1234, 1);
    let ordinary = GCBarrier {
        expiration_time: Some(end),
        ..Default::default()
    };
    let global = GlobalGCBarrier {
        expiration_time: Some(end),
        ..Default::default()
    };
    for (now, expired) in [(before, false), (end, false), (after, true)] {
        assert_eq!(ordinary.is_expired(now), expired);
        assert_eq!(global.is_expired(now), expired);
        assert!(!GCBarrier::new("forever", 0, None).is_expired(now));
        assert!(!GlobalGCBarrier::new("forever", 0, None).is_expired(now));
    }
    // A directly assigned subsecond expiration retains precision until encoding.
    let direct = GlobalGCBarrier {
        expiration_time: Some(after),
        ..global
    };
    assert!(!direct.is_expired(after));
    assert!(direct.is_expired(BarrierTime::from_unix(1234, 2)));
    assert_eq!(
        GlobalGCBarrier::from_json(&direct.to_json().unwrap())
            .unwrap()
            .expiration_time,
        Some(end)
    );
}

#[test]
fn gc_barrier_value_legacy_keyspace_and_sentinels() {
    for json in ["{}", "null", r#"{"keyspace_id":null}"#] {
        assert_eq!(
            ServiceSafePoint::from_json(json.as_bytes())
                .unwrap()
                .keyspace_id,
            u32::MAX
        );
    }
    assert_eq!(ServiceSafePoint::default().keyspace_id, 0);
    for keyspace in [0, 1, 1000, u32::MAX] {
        for seconds in [i64::MIN, -1, 0, 1, i64::MAX - 1, i64::MAX] {
            let ssp = ServiceSafePoint {
                service_id: "legacy".into(),
                safe_point: u64::MAX,
                expired_at: seconds,
                keyspace_id: keyspace,
            };
            let bytes = ssp.to_json().unwrap();
            assert_eq!(ServiceSafePoint::from_json(&bytes).unwrap(), ssp);
            assert_eq!(
                String::from_utf8(bytes).unwrap().contains("keyspace_id"),
                keyspace != u32::MAX
            );
            let barrier = GCBarrier::from_service_safe_point(&ssp);
            let finite = seconds > 0 && seconds < i64::MAX;
            assert_eq!(
                barrier.expiration_time,
                finite.then(|| BarrierTime::from_unix(seconds, 0))
            );
            assert_eq!(
                barrier.to_service_safe_point(keyspace).expired_at,
                if finite { seconds } else { i64::MAX }
            );
            assert_eq!(barrier.barrier_ts, u64::MAX);
        }
    }
    let ssp = ServiceSafePoint::from_json(br#"{"SERVICE_ID":"old","service_id":null,"expired_at":1,"EXPIRED_AT":null,"safe_point":2,"SAFE_POINT":null,"keyspace_id":0,"KEYSPACE_ID":null,"future":{"x":1}}"#).unwrap();
    assert_eq!(
        ssp,
        ServiceSafePoint {
            service_id: "old".into(),
            expired_at: 1,
            safe_point: 2,
            keyspace_id: u32::MAX
        }
    );
}

#[test]
fn gc_barrier_value_global_json_compatibility() {
    for seconds in [i64::MIN, -1, 0, 1, i64::MAX - 1, i64::MAX] {
        let barrier =
            GlobalGCBarrier::from_json(format!(r#"{{"expiration_time":{seconds}}}"#).as_bytes())
                .unwrap();
        assert_eq!(
            barrier.expiration_time,
            (seconds > 0 && seconds < i64::MAX).then(|| BarrierTime::from_unix(seconds, 0))
        );
    }
    for bytes in [b"null".as_slice(), b"{}", br#"{"expiration_time":null}"#] {
        assert_eq!(
            GlobalGCBarrier::from_json(bytes).unwrap(),
            GlobalGCBarrier::default()
        );
    }
    let value = GlobalGCBarrier::from_json(br#"{"BARRIER_ID":"x","barrier_id":null,"BARRIER_TS":18446744073709551615,"barrier_ts":null,"expiration_time":1,"EXPIRATION_TIME":null,"future":[]}"#).unwrap();
    assert_eq!(
        value,
        GlobalGCBarrier {
            barrier_id: "x".into(),
            barrier_ts: u64::MAX,
            expiration_time: None
        }
    );
}

#[test]
fn gc_barrier_value_errors_and_escaping() {
    for bad in [
        r#"{"expired_at":9223372036854775808}"#,
        r#"{"safe_point":-1}"#,
        r#"{"safe_point":18446744073709551616}"#,
        r#"{"keyspace_id":4294967296}"#,
        r#"{"keyspace_id":-1}"#,
        r#"{"safe_point":1.0}"#,
        r#"{"service_id":12}"#,
        "[]",
        "",
        "{",
        "true",
    ] {
        assert!(
            matches!(
                ServiceSafePoint::from_json(bad.as_bytes()),
                Err(EndpointError::JsonUnmarshal(_))
            ),
            "{bad}"
        );
    }
    for bad in [
        r#"{"barrier_ts":-1}"#,
        r#"{"barrier_ts":18446744073709551616}"#,
        r#"{"expiration_time":9223372036854775808}"#,
        r#"{"expiration_time":1.5}"#,
        r#"{"expiration_time":"1"}"#,
        r#"{"barrier_id":false}"#,
        "[]",
        "true",
        "",
    ] {
        assert!(
            matches!(
                GlobalGCBarrier::from_json(bad.as_bytes()),
                Err(EndpointError::JsonUnmarshal(_))
            ),
            "{bad}"
        );
    }
    let id = "<>&\u{2028}\u{2029}\\\"\n";
    let value = GCBarrier::new(id, 0, None).to_service_safe_point(0);
    let bytes = value.to_json().unwrap();
    assert!(
        String::from_utf8(bytes.clone())
            .unwrap()
            .contains(r#"\u003c\u003e\u0026\u2028\u2029"#)
    );
    assert_eq!(ServiceSafePoint::from_json(&bytes).unwrap(), value);
}
