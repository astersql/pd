// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_storage_domain::MemoryStore;
struct TestHost(Arc<MemoryStore>);
#[async_trait]
impl Host for TestHost {
    fn cluster_id(&self) -> u64 {
        79
    }
    fn store(&self) -> Option<Arc<dyn MetaStore>> {
        Some(self.0.clone())
    }
    async fn is_serving(&self) -> bool {
        true
    }
}
#[tokio::test]
async fn mcs_066_previous_lease_failure_and_point_delete() {
    let store = Arc::new(MemoryStore::default());
    let cancel = CancellationToken::new();
    let service = Service::new(Arc::new(TestHost(store.clone())), cancel.clone());
    let request = p::PutRequest {
        key: b"k".to_vec(),
        value: b"one".to_vec(),
        prev_kv: true,
        ..Default::default()
    };
    let created = service
        .put(Request::new(request.clone()))
        .await
        .unwrap()
        .into_inner();
    assert!(created.prev_kv.is_none());
    let replaced = service
        .put(Request::new(p::PutRequest {
            value: b"two".to_vec(),
            ..request.clone()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(replaced.prev_kv.unwrap().value, b"one");
    let failed = service
        .put(Request::new(p::PutRequest {
            lease: 777,
            value: b"bad".to_vec(),
            ..request
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(failed.header.unwrap().error.is_some());
    assert_eq!(
        store
            .read(Read::range(b"k", b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries[0]
            .value,
        b"two"
    );
    let deleted = service
        .delete(Request::new(p::DeleteRequest {
            key: b"k".to_vec(),
            range_end: vec![0],
            prev_kv: true,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(deleted.deleted, 0);
    assert_eq!(deleted.prev_kvs[0].value, b"two");
    assert_eq!(deleted.prev_kvs[0].mod_revision, 0);
    cancel.cancel();
    assert_eq!(
        service
            .delete(Request::new(p::DeleteRequest::default()))
            .await
            .unwrap_err()
            .message(),
        "not leader"
    );
}
