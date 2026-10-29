// Copyright 2026 AsterSQL.
use astersql_pd_proto::{
    metapb,
    pdpb::{self, pd_client::PdClient},
};
type Client = PdClient<tonic::transport::Channel>;
fn header(id: u64) -> Option<pdpb::RequestHeader> {
    Some(pdpb::RequestHeader {
        cluster_id: id,
        ..Default::default()
    })
}
pub async fn observe(client: &mut Client, id: u64) -> Vec<String> {
    let mut result = vec![];
    for count in [0, 1, 5, u32::MAX] {
        let mut put = client
            .put_cluster_config(pdpb::PutClusterConfigRequest {
                header: header(id),
                cluster: Some(metapb::Cluster {
                    id,
                    max_peer_count: count,
                }),
            })
            .await
            .unwrap()
            .into_inner();
        let h = put.header.as_mut().unwrap();
        assert_eq!(h.cluster_id, id);
        h.cluster_id = 0;
        result.push(format!("{put:?}"));
        let mut get = client
            .get_cluster_config(pdpb::GetClusterConfigRequest { header: header(id) })
            .await
            .unwrap()
            .into_inner();
        let h = get.header.as_mut().unwrap();
        assert_eq!(h.cluster_id, id);
        h.cluster_id = 0;
        let cluster = get.cluster.as_mut().unwrap();
        assert_eq!(cluster.id, id);
        assert_eq!(cluster.max_peer_count, count);
        cluster.id = 0;
        result.push(format!("{get:?}"));
    }
    for cluster in [
        None,
        Some(metapb::Cluster {
            id: 7,
            max_peer_count: 9,
        }),
        Some(metapb::Cluster::default()),
    ] {
        let mut put = client
            .put_cluster_config(pdpb::PutClusterConfigRequest {
                header: header(id),
                cluster,
            })
            .await
            .unwrap()
            .into_inner();
        let h = put.header.as_mut().unwrap();
        assert_eq!(h.cluster_id, id);
        h.cluster_id = 0;
        let error = h.error.as_mut().unwrap();
        error.message = error.message.replace(
            &format!("mismatch cluster id {id}"),
            "mismatch cluster id CLUSTER",
        );
        result.push(format!("{put:?}"));
    }
    let error = client
        .get_cluster_config(pdpb::GetClusterConfigRequest {
            header: header(id + 1),
        })
        .await
        .unwrap_err();
    result.push(format!("{:?}", error.code()));
    result
}
pub async fn observe_follower(client: &mut Client, id: u64) -> Vec<String> {
    let get = client
        .get_cluster_config(pdpb::GetClusterConfigRequest { header: header(id) })
        .await
        .unwrap_err();
    let put = client
        .put_cluster_config(pdpb::PutClusterConfigRequest {
            header: header(id),
            cluster: Some(metapb::Cluster {
                id,
                max_peer_count: 3,
            }),
        })
        .await
        .unwrap_err();
    vec![
        format!("{:?}: {}", get.code(), get.message()),
        format!("{:?}: {}", put.code(), put.message()),
    ]
}
