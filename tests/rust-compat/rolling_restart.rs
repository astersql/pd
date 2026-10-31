// Copyright 2026 AsterSQL.
use crate::harness::Server;
use astersql_pd_proto::pdpb::{self, pd_client::PdClient};
use std::{path::Path, time::Duration};
pub(crate) async fn wait_leader(
    clients: &mut [PdClient<tonic::transport::Channel>],
    alive: &[bool],
) -> (usize, pdpb::GetMembersResponse) {
    tokio::time::timeout(Duration::from_secs(40), async {
        loop {
            for (i, client) in clients.iter_mut().enumerate() {
                if !alive[i] {
                    continue;
                }
                if let Ok(reply) = client.get_members(pdpb::GetMembersRequest::default()).await {
                    let reply = reply.into_inner();
                    if let Some(leader) = &reply.leader {
                        if reply
                            .members
                            .iter()
                            .any(|m| m.member_id == leader.member_id)
                        {
                            // Members ordering is not a node index; names encode the stable test index.
                            let index: usize = leader
                                .name
                                .trim_start_matches("rolling98-")
                                .parse()
                                .unwrap();
                            if alive[index] && index == i {
                                let probe = client
                                    .alloc_id(pdpb::AllocIdRequest {
                                        header: Some(pdpb::RequestHeader {
                                            cluster_id: reply.header.as_ref().unwrap().cluster_id,
                                            ..Default::default()
                                        }),
                                        ..Default::default()
                                    })
                                    .await;
                                if probe.is_ok_and(|r| {
                                    r.into_inner().header.is_some_and(|h| h.error.is_none())
                                }) {
                                    return (index, reply);
                                }
                            }
                        }
                    }
                }
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("leader election timeout")
}
pub(crate) fn launch(binary: &Path, args: &[String]) -> Server {
    Server::start_with_args(binary, &args.iter().map(String::as_str).collect::<Vec<_>>()).unwrap()
}
