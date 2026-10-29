// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

fn request(
    url: &str,
    method: &str,
    body: &str,
    content_type: &str,
) -> (u16, String, serde_json::Value) {
    let out = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "-H",
            &format!("Content-Type: {content_type}"),
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            url,
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let mut fields = text.rsplitn(3, '\n');
    let content = fields.next().unwrap().to_string();
    let status = fields.next().unwrap().parse().unwrap();
    let body = fields.next().unwrap();
    (
        status,
        content,
        serde_json::from_str(body).unwrap_or_else(|_| serde_json::Value::String(body.into())),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_038_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let mut node = PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
        .start(
            store.clone(),
            NodeConfig {
                cluster_id: 3800,
                member_id: 1,
                name: "http".into(),
                client_url: url.clone(),
                peer_url: "http://peer".into(),
            },
            listener,
            Arc::new(prometheus::Registry::new()),
        )
        .await
        .unwrap();
    node.ready().await.unwrap();
    for path in ["health", "members"] {
        for body in ["", "null", "{bad"] {
            let (status, content, value) = request(
                &format!("{url}/pd/api/v1/{path}?unused=%zz"),
                "GET",
                body,
                "text/plain",
            );
            assert_eq!(status, 200);
            assert_eq!(content, "application/json; charset=UTF-8");
            assert!(!value.is_null());
        }
    }
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let read = || Read::prefix(b"/pd/3800/raft", Consistency::Linearizable);
    let before = store.read(read()).await.unwrap().entries;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let follower_url = format!("http://{}", listener.local_addr().unwrap());
    let mut follower =
        PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
            .start(
                store.clone(),
                NodeConfig {
                    cluster_id: 3800,
                    member_id: 2,
                    name: "follower".into(),
                    client_url: follower_url.clone(),
                    peer_url: "http://peer2".into(),
                },
                listener,
                Arc::new(prometheus::Registry::new()),
            )
            .await
            .unwrap();
    follower.ready().await.unwrap();
    for path in ["health", "members"] {
        let (status, content, value) =
            request(&format!("{follower_url}/pd/api/v1/{path}"), "GET", "", "");
        assert_eq!(status, 503);
        assert_eq!(content, "application/json; charset=UTF-8");
        assert_eq!(value, "service unavailable");
    }
    assert_eq!(before, store.read(read()).await.unwrap().entries);
    follower.shutdown().await.unwrap();
    node.shutdown().await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_038_json_and_parameters() {
    use crate::http_contract::*;
    use axum::{Router, body::Body, http::StatusCode, response::IntoResponse, routing::post};
    // Test-only ingress exercises the reusable parser over TCP. It is not a
    // production PD route and does not claim to implement member management.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let cancel = CancellationToken::new();
    let stop = cancel.clone();
    let task = tokio::spawn(async move {
        axum::serve(
            listener,
            Router::new().route(
                "/",
                post(|body: Body| async move {
                    match read_json_object(body).await {
                        Ok(_) => json_response(StatusCode::OK, &"success"),
                        Err(error) => error.into_response(),
                    }
                }),
            ),
        )
        .with_graceful_shutdown(stop.cancelled_owned())
        .await
        .unwrap();
    });
    for content in ["application/json", "text/plain", ""] {
        for body in ["null", "{}", "{\"unknown\":null}", "{\"a\":1,\"a\":2}"] {
            let (status, content, value) = request(&url, "POST", body, content);
            assert_eq!(status, 200, "{body}");
            assert_eq!(content, "application/json; charset=UTF-8");
            assert_eq!(value, "success");
        }
        for body in [
            "",
            " ",
            "{",
            "{}{}",
            "[]",
            "1",
            "true",
            "\"x\"",
            "{\"a\":NaN}",
            "{\"a\":1e999}",
        ] {
            let (status, _, value) = request(&url, "POST", body, content);
            assert_eq!(status, 400, "{body}");
            assert_eq!(value["code"], "input");
            assert!(value["data"].is_null());
            assert!(value.get("stack").is_none());
        }
    }
    // A disconnected partial body must not strand the server or block shutdown.
    use tokio::io::AsyncWriteExt;
    let mut stream = tokio::net::TcpStream::connect(url.trim_start_matches("http://"))
        .await
        .unwrap();
    stream
        .write_all(b"POST / HTTP/1.1\r\nHost: localhost\r\nContent-Length: 100\r\n\r\n{")
        .await
        .unwrap();
    drop(stream);
    assert_eq!(request(&url, "POST", "{}", "").0, 200);
    cancel.cancel();
    tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();

    assert_eq!(
        parse_uint64_field(Some("18446744073709551615"), "id"),
        Ok(u64::MAX)
    );
    assert_eq!(parse_uint64_field(Some("000"), "id"), Ok(0));
    for value in [
        None,
        Some(""),
        Some("+1"),
        Some("-1"),
        Some(" 1"),
        Some("1_0"),
        Some("0x10"),
        Some("18446744073709551616"),
    ] {
        assert!(parse_uint64_field(value, "id").is_err());
    }
    assert_eq!(query_unescape("a+%2B%252F%00%FF").unwrap(), b"a +%2F\0\xff");
    for value in ["%", "%0", "%zz", "a%0g"] {
        assert!(query_unescape(value).is_err());
    }
    let body = Body::from_stream(futures_util::stream::once(async {
        Err::<Vec<u8>, _>(std::io::Error::other(
            "secret token and internal backend address",
        ))
    }));
    let error = read_json_object(body).await.unwrap_err();
    assert_eq!(error, HttpError::Internal);
    let response = error.into_response();
    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(response.headers()["tidb-error-code"], "internal");
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
        serde_json::json!({"code":"internal","msg":"internal server error","data":null})
    );
}

#[tokio::test]
async fn task_038_go_json_values() {
    use crate::http_contract::read_json_object;
    use axum::body::Body;
    for (text, expected) in [
        (r#"{"a":"\ud800"}"#, "�"),
        (r#"{"a":"\udc00"}"#, "�"),
        (r#"{"a":"\ud83d\ude00"}"#, "😀"),
        (r#"{"a":"\\ud800"}"#, r"\ud800"),
    ] {
        assert_eq!(
            read_json_object(Body::from(text)).await.unwrap().unwrap()["a"],
            expected
        );
    }
    let raw = b"{\"a\":\"\xff\"}".to_vec();
    assert_eq!(
        read_json_object(Body::from(raw)).await.unwrap().unwrap()["a"],
        "�"
    );
    let map = read_json_object(Body::from(
        r#"{"a":9007199254740993,"b":null,"a":9007199254740993}"#,
    ))
    .await
    .unwrap()
    .unwrap();
    assert_eq!(map["a"].as_f64(), Some(9007199254740992.0));
    assert!(map["b"].is_null());
    assert!(
        read_json_object(Body::from("null"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
#[ignore = "requires freshly captured Go oracle via http-contract-oracle.py"]
async fn task_038_readonly_go_differential() {
    use crate::http_contract::*;
    use axum::{body::Body, http::StatusCode, response::IntoResponse};
    let path = std::env::var("PD_HTTP_CONTRACT_ORACLE").expect("run read-only oracle supervisor");
    let rows: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    for row in rows["json"].as_array().unwrap() {
        let body = row["input"].as_str().unwrap();
        let response = match read_json_object(Body::from(body.to_owned())).await {
            Ok(_) => json_response(StatusCode::OK, &"success"),
            Err(error) => error.into_response(),
        };
        assert_eq!(
            response.status().as_u16() as u64,
            row["status"].as_u64().unwrap(),
            "{body}"
        );
        assert_eq!(
            response.headers()["content-type"].to_str().unwrap(),
            row["headers"]["Content-Type"].as_str().unwrap()
        );
        if response.status() == StatusCode::BAD_REQUEST {
            assert_eq!(
                response.headers()["tidb-error-code"].to_str().unwrap(),
                row["headers"]["Tidb-Error-Code"].as_str().unwrap()
            );
            // Input diagnostics are deliberately normalized, not parser-specific
            // Go reflection/offset data. Compare the stable classification.
            let original: serde_json::Value =
                serde_json::from_str(row["body"].as_str().unwrap()).unwrap();
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            let observed: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(observed["code"], original["code"]);
            assert!(observed.get("stack").is_none());
        } else {
            let bytes = axum::body::to_bytes(response.into_body(), 4096)
                .await
                .unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&bytes).unwrap(),
                serde_json::from_str::<serde_json::Value>(row["body"].as_str().unwrap()).unwrap()
            );
        }
    }
    for row in rows["uint"].as_array().unwrap() {
        let HttpError::Input(message) =
            parse_uint64_field(Some(row["input"].as_str().unwrap()), "id").unwrap_err()
        else {
            panic!("input error")
        };
        assert_eq!(row["status"], 400);
        assert_eq!(
            serde_json::from_str::<String>(row["body"].as_str().unwrap()).unwrap(),
            message
        );
    }
    assert_eq!(
        rows["before"], rows["after"],
        "oracle requests must not change membership"
    );
    assert!(rows["json"].as_array().unwrap().len() >= 45);
}
