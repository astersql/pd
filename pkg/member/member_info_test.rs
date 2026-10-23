// Copyright 2026 AsterSQL.
use super::{MemberInfo, MemberInfoError};
use astersql_pd_storage_kv::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
#[allow(dead_code)]
#[path = "../storage/kv/etcd_fixture_test.rs"]
mod fixture;

async fn behavior(store: Arc<dyn MetaStore>) {
    let cancel = CancellationToken::new();
    let info = MemberInfo::new(store.clone(), cancel.clone(), 42);
    for result in [
        info.get_member_deploy_path(7).await,
        info.get_member_binary_version(7).await,
        info.get_member_git_hash(7).await,
    ] {
        assert_eq!(result, Err(MemberInfoError::NoValue));
    }
    // Similar prefix keys must not masquerade as an exact-key value.
    store
        .transact(
            vec![],
            vec![Mutation::put(
                b"/pd/42/member/7/git_hash/extra",
                b"neighbor",
            )],
        )
        .await
        .unwrap();
    assert_eq!(
        info.get_member_git_hash(7).await,
        Err(MemberInfoError::NoValue)
    );
    info.set_member_deploy_path(7, b"/opt/pd dir")
        .await
        .unwrap();
    info.set_member_binary_version(7, b"v8.0-dev\0raw")
        .await
        .unwrap();
    info.set_member_git_hash(7, &[0xff, 0x00, 0x01])
        .await
        .unwrap();
    assert_eq!(
        info.get_member_deploy_path(7).await.unwrap(),
        b"/opt/pd dir"
    );
    assert_eq!(
        info.get_member_binary_version(7).await.unwrap(),
        b"v8.0-dev\0raw"
    );
    assert_eq!(info.get_member_git_hash(7).await.unwrap(), [0xff, 0, 1]);
    for (suffix, value) in [
        ("deploy_path", b"/opt/pd dir".as_slice()),
        ("binary_version", b"v8.0-dev\0raw"),
        ("git_hash", &[0xff, 0, 1]),
    ] {
        let key = format!("/pd/42/member/7/{suffix}");
        let rows = store
            .read(Read::range(key.as_bytes(), b"", Consistency::Linearizable))
            .await
            .unwrap()
            .entries;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, value);
        assert_eq!(rows[0].lease, None);
    }
    info.set_member_git_hash(8, b"other").await.unwrap();
    let other_cluster = MemberInfo::new(store.clone(), CancellationToken::new(), 43);
    other_cluster
        .set_member_git_hash(7, b"cluster43")
        .await
        .unwrap();
    info.set_member_git_hash(7, b"").await.unwrap();
    info.set_member_binary_version(7, b"").await.unwrap();
    info.set_member_deploy_path(7, b"").await.unwrap();
    assert_eq!(info.get_member_git_hash(7).await.unwrap(), b"");
    assert_eq!(info.get_member_binary_version(7).await.unwrap(), b"");
    assert_eq!(info.get_member_deploy_path(7).await.unwrap(), b"");
    assert_eq!(info.get_member_git_hash(8).await.unwrap(), b"other");
    assert_eq!(
        other_cluster.get_member_git_hash(7).await.unwrap(),
        b"cluster43"
    );
    cancel.cancel();
    for result in [
        info.get_member_git_hash(7).await,
        info.get_member_deploy_path(7).await,
        info.get_member_binary_version(7).await,
    ] {
        assert_eq!(result, Err(MemberInfoError::Cancelled));
    }
    for result in [
        info.set_member_git_hash(8, b"bad").await,
        info.set_member_deploy_path(8, b"bad").await,
        info.set_member_binary_version(8, b"bad").await,
    ] {
        assert_eq!(result, Err(MemberInfoError::Cancelled));
    }
    let fresh = MemberInfo::new(store, CancellationToken::new(), 42);
    assert_eq!(fresh.get_member_git_hash(8).await.unwrap(), b"other");
    assert_eq!(
        fresh.get_member_deploy_path(8).await,
        Err(MemberInfoError::NoValue)
    );
    assert_eq!(
        fresh.get_member_binary_version(8).await,
        Err(MemberInfoError::NoValue)
    );
}
#[tokio::test]
async fn member_info_memory() {
    behavior(Arc::new(MemoryStore::default())).await;
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn member_info_etcd() {
    let fixture = fixture::Fixture::start();
    behavior(Arc::new(fixture.store().await)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn member_info_etcd_io_failure_and_cancellation() {
    let mut fixture = fixture::Fixture::start();
    let store = Arc::new(fixture.store().await);
    let cancel = CancellationToken::new();
    let info = MemberInfo::new(store.clone(), cancel.clone(), 44);
    info.set_member_deploy_path(7, b"/original").await.unwrap();
    info.set_member_binary_version(7, b"old-version")
        .await
        .unwrap();
    info.set_member_git_hash(7, b"old-hash").await.unwrap();
    fixture.command("stop", "stopped");
    eprintln!(
        "member_info: etcd stopped after acknowledged writes; no server during failing requests"
    );
    for result in [
        info.set_member_deploy_path(7, b"bad").await,
        info.set_member_binary_version(7, b"bad").await,
        info.set_member_git_hash(7, b"bad").await,
    ] {
        assert_eq!(result, Err(MemberInfoError::Store(StoreError::Unavailable)));
    }
    assert_eq!(
        info.get_member_git_hash(7).await,
        Err(MemberInfoError::Store(StoreError::Unavailable))
    );
    fixture.command("start", "restarted");
    let fresh = MemberInfo::new(
        Arc::new(fixture.store().await),
        CancellationToken::new(),
        44,
    );
    assert_eq!(fresh.get_member_deploy_path(7).await.unwrap(), b"/original");
    assert_eq!(
        fresh.get_member_binary_version(7).await.unwrap(),
        b"old-version"
    );
    assert_eq!(fresh.get_member_git_hash(7).await.unwrap(), b"old-hash");
    eprintln!("member_info: same WAL restarted; all original values retained");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn member_info_pending_io_cancel() {
    // A real TCP boundary accepts a connection but withholds the HTTP/2 reply.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stopped = CancellationToken::new();
    let shutdown = stopped.clone();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        shutdown.cancelled().await;
        drop(socket);
    });
    let store = EtcdMetaStore::connect(&[endpoint], std::time::Duration::from_secs(2))
        .await
        .unwrap();
    let cancel = CancellationToken::new();
    let info = MemberInfo::new(Arc::new(store), cancel.clone(), 42);
    let read = info.get_member_deploy_path(7);
    tokio::pin!(read);
    tokio::select! {
        result = &mut read => panic!("request completed before cancellation: {result:?}"),
        _ = tokio::time::sleep(std::time::Duration::from_millis(50)) => {}
    }
    cancel.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_millis(500), &mut read).await;
    stopped.cancel();
    server.await.unwrap();
    assert_eq!(result.unwrap(), Err(MemberInfoError::Cancelled));
    eprintln!(
        "member_info: TCP response withheld; read pending for 50ms, cancellation terminated within 500ms"
    );
}
