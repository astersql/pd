// Copyright 2026 AsterSQL.
use astersql_pd_runtime::{NodeConfig, PdNodeBuilder};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub(crate) fn request(url: &str, method: &str, path: &str, body: &str) -> (u16, serde_json::Value) {
    let output = std::process::Command::new("curl")
        .args([
            "-sS",
            "--max-time",
            "5",
            "-X",
            method,
            "--data-binary",
            body,
            "-w",
            "\n%{http_code}\n%{content_type}",
            &format!("{url}{path}"),
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "{output:?}");
    let text = String::from_utf8(output.stdout).unwrap();
    let (text, content) = text.rsplit_once('\n').unwrap();
    assert_eq!(content, "application/json; charset=UTF-8");
    let (body, code) = text.rsplit_once('\n').unwrap();
    (
        code.parse().unwrap(),
        serde_json::from_str(body).unwrap_or(serde_json::Value::String(body.into())),
    )
}
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn task_044_behavior() {
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    for id in 1..=2 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 744,
                        member_id: id,
                        name: format!("member-{id}"),
                        client_url: url.clone(),
                        peer_url: format!("http://peer-{id}"),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
    }
    let observation = request(&urls[0], "GET", "/pd/api/v1/members", "");
    use astersql_pd_storage::{Consistency, MetaStore, Read};
    let members = Read::prefix(b"/pd/744/rust/members/", Consistency::Linearizable);
    let before = store.read(members.clone()).await.unwrap();
    assert_eq!(
        request(
            &urls[1],
            "POST",
            "/pd/api/v1/members/name/member-2",
            r#"{"leader-priority":7.9}"#
        ),
        (200, serde_json::json!("success"))
    );
    let info = request(&urls[0], "GET", "/pd/api/v1/members", "");
    assert_eq!(
        info.1["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["member_id"] == 2)
            .unwrap()["leader_priority"],
        7
    );
    assert_eq!(
        request(
            &urls[0],
            "POST",
            "/pd/api/v1/members/name/member-2",
            r#"{"leader-priority":"bad"}"#
        ),
        (400, serde_json::json!("bad format leader priority"))
    );
    assert_eq!(
        request(
            &urls[0],
            "POST",
            "/pd/api/v1/members/name/absent",
            "invalid"
        )
        .0,
        404
    );
    assert_eq!(
        request(&urls[0], "DELETE", "/pd/api/v1/members/name/absent", "").0,
        404
    );
    assert_eq!(
        request(&urls[0], "DELETE", "/pd/api/v1/members/id/no", "").0,
        400
    );
    assert_eq!(
        request(&urls[0], "DELETE", "/pd/api/v1/members/id/2", "").0,
        500
    );
    assert_eq!(before.entries, store.read(members).await.unwrap().entries);
    let priority = store
        .read(Read::range(
            b"/pd/744/member/2/leader_priority",
            b"",
            Consistency::Linearizable,
        ))
        .await
        .unwrap();
    assert_eq!(priority.entries[0].value, b"7");
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }

    assert_eq!(observation.0, 200);
    assert_eq!(
        observation.1["members"].as_array().unwrap().len(),
        2,
        "HTTP must discover both real members"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires read-only Go oracle supervisor"]
async fn task_044_readonly_go_differential() {
    let oracle: serde_json::Value = serde_json::from_slice(
        &std::fs::read(std::env::var("PD_HTTP_MEMBER_ORACLE").unwrap()).unwrap(),
    )
    .unwrap();
    let fixture = crate::etcd_fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let mut nodes = Vec::new();
    let mut urls = Vec::new();
    for id in 1..=3 {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let mut node =
            PdNodeBuilder::new(tokio::runtime::Handle::current(), CancellationToken::new())
                .start(
                    store.clone(),
                    NodeConfig {
                        cluster_id: 744,
                        member_id: id,
                        name: format!("member-{id}"),
                        client_url: url.clone(),
                        peer_url: format!("http://peer-{id}"),
                    },
                    listener,
                    Arc::new(prometheus::Registry::new()),
                )
                .await
                .unwrap();
        node.ready().await.unwrap();
        nodes.push(node);
        urls.push(url);
    }
    for (key, method, path, body) in [
        (
            "set",
            "POST",
            "/pd/api/v1/members/name/member-2",
            r#"{"leader-priority":7.9}"#,
        ),
        (
            "bad",
            "POST",
            "/pd/api/v1/members/name/member-2",
            r#"{"leader-priority":"bad"}"#,
        ),
        (
            "unknown",
            "POST",
            "/pd/api/v1/members/name/member-2",
            r#"{"ignored":true}"#,
        ),
        ("null", "POST", "/pd/api/v1/members/name/member-2", "null"),
        (
            "absent",
            "POST",
            "/pd/api/v1/members/name/absent",
            "invalid",
        ),
        (
            "delete_absent",
            "DELETE",
            "/pd/api/v1/members/name/absent",
            "",
        ),
        ("bad_id", "DELETE", "/pd/api/v1/members/id/no", ""),
    ] {
        assert_eq!(
            serde_json::json!(request(&urls[0], method, path, body)),
            oracle["responses"][key],
            "{key}"
        );
    }
    let actual = request(&urls[0], "GET", "/pd/api/v1/members", "");
    assert_eq!(actual.0, 200);
    // Normalize process identities/URLs/publication metadata only. Priority and
    // real membership/name remain exact; topology is tested separately.
    let normalized = |v: &serde_json::Value| {
        let mut members: Vec<_> = v["members"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["name"].as_str().unwrap().to_owned(),
                    m["leader_priority"].as_i64().unwrap_or(0),
                )
            })
            .collect();
        members.sort();
        members
    };
    assert_eq!(
        normalized(&actual.1),
        normalized(&oracle["responses"]["get"][1])
    );
    assert_eq!(
        actual.1["leader"]["name"],
        oracle["responses"]["get"][1]["leader"]["name"]
    );
    assert!(
        actual.1.get("etcd_leader").is_none(),
        "independent PD IDs must not fabricate an etcd leader"
    );
    for node in &mut nodes {
        node.shutdown().await.unwrap();
    }
}
