// Copyright 2026 AsterSQL.
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use astersql_pd_storage::{Consistency, MetaStore, Mutation, Read};
use prost::Message;
use serde_json::{Value, json};
use std::time::Duration;
fn item(item: pdpb::GlobalConfigItem) -> Value {
    json!({"payload": item.payload, "kind": item.kind, "wire": item.encode_to_vec()})
}
type Client = PdClient<tonic::transport::Channel>;
async fn message(
    stream: &mut tonic::Streaming<pdpb::WatchGlobalConfigResponse>,
) -> pdpb::WatchGlobalConfigResponse {
    tokio::time::timeout(Duration::from_secs(10), stream.message())
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}
pub async fn observe(client: &mut Client, store: &dyn MetaStore) -> Value {
    let cluster_id = client
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let prefix = "/global/config/task033/";
    let key = format!("{prefix}a");
    let revision = store
        .transact(vec![], vec![Mutation::put(key.as_bytes(), &[0, 255, 1])])
        .await
        .unwrap()
        .revision;
    let deleted = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: key.as_bytes().to_vec(),
            }],
        )
        .await
        .unwrap()
        .revision;
    let mut stream = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: prefix.into(),
            revision,
        })
        .await
        .unwrap()
        .into_inner();
    let mut last = revision - 1;
    // Backend may combine replayed revisions into a single watch response.
    let mut changes = vec![];
    while changes.len() < 2 {
        let batch = message(&mut stream).await;
        assert!(batch.header.is_none());
        assert!(batch.revision > last && batch.revision <= deleted);
        last = batch.revision;
        for change in batch.changes {
            changes.push(item(change));
        }
    }
    assert_eq!(last, deleted);
    assert_eq!(changes[0]["payload"], json!([0, 255, 1]));
    assert_eq!(changes[1]["payload"], json!([0, 255, 1]));
    assert_eq!(changes[1]["kind"], pdpb::EventType::Delete as i32);
    assert!(
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries
            .is_empty()
    );
    drop(stream);
    // Resume inclusively at the next revision; unrelated prefixes produce no item.
    let next = store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/global/config/task033-sibling/a",
                b"excluded",
            )],
        )
        .await
        .unwrap()
        .revision
        + 1;
    let second = format!("{prefix}b");
    let (result, commit) = tokio::join!(
        client.watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: prefix.into(),
            revision: next
        }),
        store.transact(
            vec![],
            vec![
                Mutation::put(key.as_bytes(), b"live"),
                Mutation::put(second.as_bytes(), b"second")
            ]
        )
    );
    let commit = commit.unwrap();
    let mut stream = result.unwrap().into_inner();
    let live = message(&mut stream).await;
    assert_eq!(live.revision, commit.revision);
    assert_eq!(live.changes.len(), 2);
    assert_eq!(live.changes[0].payload, b"live");
    assert_eq!(live.changes[1].payload, b"second");
    drop(stream); // Actual HTTP/2 disconnect.
    let invalid = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: "resource_group/controller".into(),
            ..Default::default()
        })
        .await
        .unwrap_err();
    assert_eq!(invalid.code(), tonic::Code::InvalidArgument);
    let invalid_message = invalid.message().to_owned();
    // Receive an initial event before the deadline, so the remote streaming
    // deadline is observed after headers rather than tonic's local unary timeout.
    let mut request = tonic::Request::new(pdpb::WatchGlobalConfigRequest {
        config_path: prefix.into(),
        revision: commit.revision,
    });
    request.set_timeout(Duration::from_millis(300));
    let mut deadline_stream = client
        .watch_global_config(request)
        .await
        .unwrap()
        .into_inner();
    let initial = message(&mut deadline_stream).await;
    assert_eq!(initial.revision, commit.revision);
    let deadline_status = tokio::time::timeout(Duration::from_secs(5), deadline_stream.message())
        .await
        .unwrap()
        .unwrap_err();
    // The Go server may close the HTTP/2 stream with remote CANCEL at expiry.
    // Preserve the raw diagnostic, and compare terminal cancellation semantics.
    assert!(matches!(
        deadline_status.code(),
        tonic::Code::DeadlineExceeded | tonic::Code::Cancelled
    ));
    assert!(
        matches!(
            deadline_status.message(),
            "context deadline exceeded"
                | "context canceled"
                | "h2 protocol error: error reading a body from connection"
        ),
        "{deadline_status:?}"
    );
    println!(
        "stream deadline status: {:?} {}",
        deadline_status.code(),
        deadline_status.message()
    );
    // Compact strictly above requested revision and inspect both message and trailers.
    store.compact(commit.revision).await.unwrap();
    let mut stream = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: prefix.into(),
            revision,
        })
        .await
        .unwrap()
        .into_inner();
    let compact = message(&mut stream).await;
    let error = compact.header.as_ref().unwrap().error.as_ref().unwrap();
    assert_eq!(error.r#type, pdpb::ErrorType::DataCompacted as i32);
    assert_eq!(
        error.message,
        format!(
            "required watch revision: {revision} is smaller than current compact/min revision {}.",
            commit.revision
        )
    );
    let status = tokio::time::timeout(Duration::from_secs(5), stream.message())
        .await
        .unwrap()
        .unwrap_err();
    println!(
        "compaction terminal status: {:?} {}",
        status.code(),
        status.message()
    );
    assert!(compact.changes.is_empty());
    assert_eq!(compact.revision, 0);
    assert_eq!(compact.header.as_ref().unwrap().cluster_id, cluster_id);
    let mut normalized_compact = compact.clone();
    normalized_compact.header.as_mut().unwrap().cluster_id = 0;
    normalized_compact
        .header
        .as_mut()
        .unwrap()
        .error
        .as_mut()
        .unwrap()
        .message = "compacted".into();
    // The boundary revision itself remains watchable; only older revisions fail.
    let mut equal = client
        .watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: prefix.into(),
            revision: commit.revision,
        })
        .await
        .unwrap()
        .into_inner();
    let mut equal_response = message(&mut equal).await;
    assert!(equal_response.header.is_none());
    assert_eq!(equal_response.revision, commit.revision);
    assert_eq!(
        equal_response
            .changes
            .iter()
            .map(Message::encode_to_vec)
            .collect::<Vec<_>>(),
        live.changes
            .iter()
            .map(Message::encode_to_vec)
            .collect::<Vec<_>>()
    );
    equal_response.revision = 0; // Asserted against the backend's real commit above.
    drop(equal);
    // A delete whose previous version was compacted is omitted, rather than
    // manufacturing an empty payload. The next live PUT still reaches the stream.
    let deleted_boundary = store
        .transact(
            vec![],
            vec![Mutation::Delete {
                key: key.as_bytes().to_vec(),
            }],
        )
        .await
        .unwrap()
        .revision;
    store.compact(deleted_boundary).await.unwrap();
    let (result, commit_after) = tokio::join!(
        client.watch_global_config(pdpb::WatchGlobalConfigRequest {
            config_path: prefix.into(),
            revision: deleted_boundary
        }),
        store.transact(
            vec![],
            vec![Mutation::put(key.as_bytes(), b"after-compaction")]
        )
    );
    let mut omitted = result.unwrap().into_inner();
    let after = message(&mut omitted).await;
    assert_eq!(after.revision, commit_after.unwrap().revision);
    assert_eq!(after.changes.len(), 1);
    assert_eq!(after.changes[0].kind, pdpb::EventType::Put as i32);
    assert_eq!(after.changes[0].payload, b"after-compaction");
    drop(omitted);
    assert_eq!(
        store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"after-compaction"
    );
    json!({"changes": changes, "live": live.changes.into_iter().map(item).collect::<Vec<_>>(), "invalid": invalid_message, "compact_kind": error.r#type, "compact_wire": normalized_compact.encode_to_vec(), "equal_wire": equal_response.encode_to_vec(), "after_compact": after.changes.into_iter().map(item).collect::<Vec<_>>(), "deadline_terminal": true, "terminal": format!("{:?}:{}", status.code(), status.message())})
}
