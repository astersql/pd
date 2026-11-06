// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{MemoryStore, Mutation};
use futures_util::StreamExt;
use std::time::Duration;
struct TestHost(Arc<MemoryStore>);
#[async_trait]
impl Host for TestHost {
    fn cluster_id(&self) -> u64 {
        71
    }
    fn store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.0.clone())
    }
    async fn is_serving(&self) -> bool {
        true
    }
}
#[tokio::test]
async fn mcs_067_point_range_previous_and_cancel() {
    let store = Arc::new(MemoryStore::default());
    let cancel = CancellationToken::new();
    let service = Service::new(Arc::new(TestHost(store.clone())), cancel.clone());
    let seed = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"a", b"first"),
                Mutation::put(b"ab", b"neighbor"),
            ],
        )
        .await
        .unwrap();
    let mut stream = service
        .watch(Request::new(p::WatchRequest {
            key: b"a".to_vec(),
            start_revision: seed.revision,
            prev_kv: true,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    let first = stream.next().await.unwrap().unwrap();
    assert_eq!(first.events.len(), 1);
    assert_eq!(first.events[0].kv.as_ref().unwrap().version, 1);
    store
        .transact(vec![], vec![Mutation::put(b"a", b"second")])
        .await
        .unwrap();
    let update = stream.next().await.unwrap().unwrap();
    assert_eq!(update.events[0].kv.as_ref().unwrap().version, 2);
    assert_eq!(update.events[0].prev_kv.as_ref().unwrap().value, b"first");
    cancel.cancel();
    assert!(stream.next().await.is_none());
    let service = Service::new(Arc::new(TestHost(store.clone())), CancellationToken::new());
    let mut range = service
        .watch(Request::new(p::WatchRequest {
            key: b"a".to_vec(),
            range_end: b"b".to_vec(),
            start_revision: seed.revision,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    let batch = range.next().await.unwrap().unwrap();
    assert_eq!(batch.events.len(), 3);
    assert!(batch.events.iter().all(|event| event.prev_kv.is_none()));
    drop(range);
    let current = service.watch(Request::new(p::WatchRequest {
        key: b"a".to_vec(),
        ..Default::default()
    }));
    tokio::pin!(current);
    assert!(
        tokio::time::timeout(Duration::from_millis(10), &mut current)
            .await
            .is_err()
    );
    store
        .transact(vec![], vec![Mutation::put(b"a", b"third")])
        .await
        .unwrap();
    let mut current = current.await.unwrap().into_inner();
    assert_eq!(
        current.next().await.unwrap().unwrap().events[0]
            .kv
            .as_ref()
            .unwrap()
            .version,
        3
    );
}
#[tokio::test]
async fn mcs_067_compaction_header_then_terminal_status() {
    let store = Arc::new(MemoryStore::default());
    let seed = store
        .transact(vec![], vec![Mutation::put(b"k", b"one")])
        .await
        .unwrap();
    let newer = store
        .transact(vec![], vec![Mutation::put(b"k", b"two")])
        .await
        .unwrap();
    store.compact(newer.revision).await.unwrap();
    let service = Service::new(Arc::new(TestHost(store)), CancellationToken::new());
    let mut stream = service
        .watch(Request::new(p::WatchRequest {
            key: b"k".to_vec(),
            start_revision: seed.revision,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    let response = stream.next().await.unwrap().unwrap();
    assert_eq!(response.compact_revision, newer.revision);
    assert_eq!(
        response.header.unwrap().error.unwrap().r#type,
        p::ErrorType::DataCompacted as i32
    );
    assert_eq!(
        stream.next().await.unwrap().unwrap_err().code(),
        tonic::Code::Unknown
    );
    assert!(stream.next().await.is_none());
}
