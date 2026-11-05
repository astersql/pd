// Copyright 2026 AsterSQL.
use astersql_pd_proto::{pdpb, tsopb as p};
use astersql_pd_tso::GroupAllocatorWatch;
use futures_util::Stream;
use std::{pin::Pin, sync::Arc};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};
#[derive(Clone)]
pub(crate) struct Service {
    pub watch: Arc<Mutex<GroupAllocatorWatch>>,
    pub cluster: u64,
    pub store: Arc<dyn astersql_pd_storage_domain::MetaStore>,
    pub cancel: CancellationToken,
}
fn header(cluster: u64, group: u32) -> p::ResponseHeader {
    p::ResponseHeader {
        cluster_id: cluster,
        keyspace_group_id: group,
        ..Default::default()
    }
}
impl Service {
    fn validate(&self, h: Option<&p::RequestHeader>) -> Result<(), Status> {
        let h = h.ok_or_else(|| Status::invalid_argument("missing request header"))?;
        if h.cluster_id != self.cluster {
            return Err(Status::failed_precondition("mismatch cluster id"));
        }
        Ok(())
    }
}
#[tonic::async_trait]
impl p::tso_server::Tso for Service {
    async fn tso(
        &self,
        request: Request<tonic::Streaming<p::TsoRequest>>,
    ) -> Result<Response<Pin<Box<dyn Stream<Item = Result<p::TsoResponse, Status>> + Send>>>, Status>
    {
        let mut stream = request.into_inner();
        let service = self.clone();
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            loop {
                let next = tokio::select! {biased;_=service.cancel.cancelled()=>break,r=stream.message()=>r};
                let request = match next {
                    Ok(Some(r)) => r,
                    Ok(None) => break,
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                        break;
                    }
                };
                let result = async {
                    service.validate(request.header.as_ref())?;
                    let h = request.header.unwrap();
                    let w = service.watch.lock().await;
                    let key = match h.keyspace {
                        Some(p::request_header::Keyspace::KeyspaceId(id)) => id,
                        Some(_) => {
                            return Err(Status::invalid_argument("unsupported scoped keyspace"));
                        }
                        None => 0,
                    };
                    let group = w
                        .loaded_ids()
                        .into_iter()
                        .find(|id| {
                            w.group(*id).is_some_and(|g| {
                                g.keyspaces.as_ref().is_some_and(|keys| keys.contains(&key))
                            })
                        })
                        .ok_or_else(|| Status::not_found("keyspace not assigned"))?;
                    let a = w
                        .allocator(group)
                        .ok_or_else(|| Status::unavailable("not primary"))?;
                    let t = a
                        .allocate(request.count)
                        .await
                        .map_err(|e| Status::unavailable(e.to_string()))?;
                    Ok(p::TsoResponse {
                        header: Some(header(service.cluster, group)),
                        count: request.count,
                        timestamp: Some(pdpb::Timestamp {
                            physical: t.physical as i64,
                            logical: t.logical as i64,
                            ..Default::default()
                        }),
                    })
                }
                .await;
                tokio::select! {_=service.cancel.cancelled()=>break,r=tx.send(result)=>{if r.is_err(){break;}}}
            }
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
    async fn find_group_by_keyspace_id(
        &self,
        request: Request<p::FindGroupByKeyspaceIdRequest>,
    ) -> Result<Response<p::FindGroupByKeyspaceIdResponse>, Status> {
        let r = request.into_inner();
        self.validate(r.header.as_ref())?;
        let key = match r.keyspace {
            Some(p::find_group_by_keyspace_id_request::Keyspace::KeyspaceId(id)) => id,
            _ => 0,
        };
        let w = self.watch.lock().await;
        let g = w
            .loaded_ids()
            .into_iter()
            .filter_map(|id| w.group(id))
            .find(|g| g.keyspaces.as_ref().is_some_and(|ids| ids.contains(&key)))
            .ok_or_else(|| Status::not_found("keyspace not assigned"))?
            .clone();
        let revision = w.revision();
        drop(w);
        use astersql_pd_storage_domain::{Consistency, Read};
        use prost::Message;
        let election = astersql_pd_keypath::ClusterPaths::new(self.cluster).election_path(Some(
            &astersql_pd_keypath::MsParam {
                service_name: "tso".into(),
                group_id: g.id,
            },
        ));
        let snapshot = self
            .store
            .read(Read::prefix(election.as_bytes(), Consistency::Linearizable))
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        let primary = snapshot
            .entries
            .iter()
            .find(|e| e.key == election.as_bytes())
            .and_then(|e| p::Participant::decode(e.value.as_slice()).ok())
            .and_then(|p| p.listen_urls.first().cloned());
        Ok(Response::new(p::FindGroupByKeyspaceIdResponse {
            header: Some(header(self.cluster, g.id)),
            mod_revision: revision as u64,
            keyspace_group: Some(p::KeyspaceGroup {
                id: g.id,
                user_kind: g.user_kind.clone(),
                split_state: g.split_state.as_ref().map(|s| p::SplitState {
                    split_source: s.split_source,
                }),
                members: g
                    .members
                    .iter()
                    .flatten()
                    .map(|m| p::KeyspaceGroupMember {
                        address: m.address.clone(),
                        is_primary: primary.as_ref().is_some_and(|p| m.is_address_equivalent(p)),
                    })
                    .collect(),
            }),
        }))
    }
    async fn get_min_ts(
        &self,
        request: Request<p::GetMinTsRequest>,
    ) -> Result<Response<p::GetMinTsResponse>, Status> {
        self.validate(request.into_inner().header.as_ref())?;
        let w = self.watch.lock().await;
        let mut minimum = None;
        let mut serving = 0;
        for id in w.assigned_ids() {
            if let Ok(t) = w.allocator(id).unwrap().allocate(1).await {
                serving += 1;
                minimum = Some(minimum.map_or(t, |old: astersql_pd_tso::Timestamp| old.min(t)));
            }
        }
        Ok(Response::new(p::GetMinTsResponse {
            header: Some(header(self.cluster, 0)),
            timestamp: minimum.map(|t| pdpb::Timestamp {
                physical: t.physical as i64,
                logical: t.logical as i64,
                ..Default::default()
            }),
            keyspace_groups_serving: serving,
            keyspace_groups_total: w.loaded_ids().len() as u32,
        }))
    }
}
