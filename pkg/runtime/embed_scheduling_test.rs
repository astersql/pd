// Copyright 2026 AsterSQL.
use super::*;
use astersql_pd_compat_tests::{harness::Server, readonly_oracle::pinned_go_binary};
use astersql_pd_proto::pdpb;
#[tokio::test]
async fn mcs_139_remote_planner_ids_use_supplied_blocking_context_on_single_thread_host() {
    let go = pinned_go_binary().unwrap();
    let mut pd = Server::start(&go).unwrap();
    pd.ready().unwrap();
    let mut owner = pdpb::pd_client::PdClient::connect(pd.url.clone())
        .await
        .unwrap();
    let cluster = owner
        .get_members(pdpb::GetMembersRequest::default())
        .await
        .unwrap()
        .into_inner()
        .header
        .unwrap()
        .cluster_id;
    let header = Some(pdpb::RequestHeader {
        cluster_id: cluster,
        ..Default::default()
    });
    let baseline = owner
        .alloc_id(pdpb::AllocIdRequest {
            header: header.clone(),
            count: 1,
        })
        .await
        .unwrap()
        .into_inner()
        .id;
    let backend = astersql_pd_storage::EtcdMetaStore::connect(
        &[pd.metadata_url().into()],
        Duration::from_secs(2),
    )
    .await
    .unwrap();
    let lookup: PdLeaderLookup = Arc::new(move || {
        let backend = backend.clone();
        Box::pin(async move {
            backend
                .consensus_leader_client_url()
                .await
                .map_err(|e| e.to_string())
        })
    });
    let runtime = Handle::current();
    let cancel = CancellationToken::new();
    let delegates = Arc::new(DelegateConnections::new(runtime.clone(), &cancel, None));
    let ids = Arc::new(RemoteIds::new(
        cluster,
        delegates.clone(),
        cancel.clone(),
        Arc::new(Notify::new()),
        runtime.clone(),
    ));
    let missing = ids.alloc_count(1).await.unwrap_err();
    assert_eq!(missing.message(), "PD leader is not found");
    ids.refresh(&lookup).await.unwrap();
    let supplied = runtime.clone();
    let allocation = runtime
        .spawn_blocking(move || supplied.block_on(async move { ids.alloc_one_blocking() }))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(allocation, baseline + 1);
    let after = owner
        .alloc_id(pdpb::AllocIdRequest { header, count: 1 })
        .await
        .unwrap()
        .into_inner()
        .id;
    assert_eq!(after, baseline + 2);
    let path = std::path::PathBuf::from(std::env::var_os("PD_SA016_EVIDENCE").unwrap())
        .with_extension("scheduling-blocking.json");
    std::fs::write(path,serde_json::to_vec(&serde_json::json!({"host":"single-thread Tokio with caller blocking handle","before":baseline,"allocated":allocation,"after":after,"missing":missing.message()})).unwrap()).unwrap();
    cancel.cancel();
    delegates.shutdown();
    pd.stop().unwrap();
}
