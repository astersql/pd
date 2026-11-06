// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::{MemoryStore, Mutation};
use std::sync::atomic::{AtomicBool, Ordering};
struct TestHost {
    store: Arc<MemoryStore>,
    serving: AtomicBool,
}
#[async_trait]
impl Host for TestHost {
    fn cluster_id(&self) -> u64 {
        73
    }
    fn store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.store.clone())
    }
    async fn is_serving(&self) -> bool {
        self.serving.load(Ordering::SeqCst)
    }
}
#[tokio::test]
async fn mcs_065_range_count_and_live_admission() {
    let store = Arc::new(MemoryStore::default());
    let seed = store
        .transact(
            vec![],
            vec![
                Mutation::put(b"k/a", b"one"),
                Mutation::put(b"k/b", b"two"),
                Mutation::put(b"z", b"last"),
            ],
        )
        .await
        .unwrap();
    let host = Arc::new(TestHost {
        store,
        serving: AtomicBool::new(true),
    });
    let service = Service::new(host.clone(), CancellationToken::new());
    let response = service
        .get(Request::new(p::GetRequest {
            key: b"k/".to_vec(),
            range_end: b"k0".to_vec(),
            limit: 1,
            header: Some(p::RequestHeader {
                cluster_id: 999,
                ..Default::default()
            }),
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(response.count, 2);
    assert!(response.more);
    assert_eq!(response.kvs.len(), 1);
    assert_eq!(response.kvs[0].key, b"k/a");
    assert_eq!(response.header.unwrap().revision, seed.revision);
    let all = service
        .get(Request::new(p::GetRequest {
            key: b"k/b".to_vec(),
            range_end: vec![0],
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(all.count, 2);
    assert_eq!(all.kvs[1].key, b"z");
    host.serving.store(false, Ordering::SeqCst);
    let error = service
        .get(Request::new(p::GetRequest::default()))
        .await
        .unwrap_err();
    assert_eq!(error.code(), tonic::Code::Unavailable);
    assert_eq!(error.message(), "not leader");
}
