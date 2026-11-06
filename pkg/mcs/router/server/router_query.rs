// Copyright 2026 AsterSQL.
use astersql_pd_core::{BasicCluster, BatchScanRegionsOptions, KeyRange, NodeState};
use astersql_pd_grpcutil::{
    cluster_read,
    core_store_adapter::{store_stats_to_proto, store_to_proto},
};
use astersql_pd_proto::{pdpb, routerpb::router_server::Router};
use std::sync::{Arc, Mutex};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};
/// All methods read the same cache supplied by the embedding host. This service
/// owns no PD election, process runtime, or parallel region indexes.
#[derive(Clone)]
pub struct Service {
    cluster: Arc<Mutex<BasicCluster>>,
    cluster_id: u64,
    cancel: CancellationToken,
}
impl Service {
    pub fn new(
        cluster_id: u64,
        cluster: Arc<Mutex<BasicCluster>>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            cluster,
            cluster_id,
            cancel,
        }
    }
    fn header(&self) -> pdpb::ResponseHeader {
        pdpb::ResponseHeader {
            cluster_id: self.cluster_id,
            ..Default::default()
        }
    }
    fn point(
        &self,
        key: Option<&[u8]>,
        id: u64,
        previous: bool,
        buckets: bool,
    ) -> pdpb::GetRegionResponse {
        let cluster = self.cluster.lock().unwrap();
        let region = match key {
            Some(key) => cluster.regions.region_by_key(key, previous),
            None => cluster.regions.get_region(id),
        };
        cluster_read::point(self.header(), region.as_deref(), buckets)
    }
}
#[tonic::async_trait]
impl Router for Service {
    async fn get_region(
        &self,
        request: Request<pdpb::GetRegionRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        let r = request.into_inner();
        Ok(Response::new(self.point(
            Some(&r.region_key),
            0,
            false,
            r.need_buckets,
        )))
    }
    async fn get_prev_region(
        &self,
        request: Request<pdpb::GetRegionRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        let r = request.into_inner();
        Ok(Response::new(self.point(
            Some(&r.region_key),
            0,
            true,
            r.need_buckets,
        )))
    }
    async fn get_region_by_id(
        &self,
        request: Request<pdpb::GetRegionByIdRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        let r = request.into_inner();
        Ok(Response::new(self.point(
            None,
            r.region_id,
            false,
            r.need_buckets,
        )))
    }
    async fn get_store(
        &self,
        request: Request<pdpb::GetStoreRequest>,
    ) -> Result<Response<pdpb::GetStoreResponse>, Status> {
        let id = request.into_inner().store_id;
        let cluster = self.cluster.lock().unwrap();
        let mut response = pdpb::GetStoreResponse {
            header: Some(self.header()),
            ..Default::default()
        };
        if let Some(store) = cluster.stores.get_store(id) {
            response.store = store.meta().cloned().map(store_to_proto);
            response.stats = Some(store_stats_to_proto(store.store_stats()));
        } else {
            response.header.as_mut().unwrap().error = Some(pdpb::Error {
                r#type: pdpb::ErrorType::Unknown as i32,
                message: format!("invalid store ID {id}, not found"),
            });
        }
        Ok(Response::new(response))
    }
    async fn get_all_stores(
        &self,
        request: Request<pdpb::GetAllStoresRequest>,
    ) -> Result<Response<pdpb::GetAllStoresResponse>, Status> {
        let exclude = request.into_inner().exclude_tombstone_stores;
        let stores = self
            .cluster
            .lock()
            .unwrap()
            .stores
            .get_meta_stores()
            .into_iter()
            .flatten()
            .filter(|s| !exclude || s.node_state != NodeState::Removed)
            .map(|s| store_to_proto((*s).clone()))
            .collect();
        Ok(Response::new(pdpb::GetAllStoresResponse {
            header: Some(self.header()),
            stores,
        }))
    }
    async fn batch_scan_regions(
        &self,
        request: Request<pdpb::BatchScanRegionsRequest>,
    ) -> Result<Response<pdpb::BatchScanRegionsResponse>, Status> {
        let r = request.into_inner();
        let mut response = pdpb::BatchScanRegionsResponse {
            header: Some(self.header()),
            ..Default::default()
        };
        for (i, range) in r.ranges.iter().enumerate() {
            let message = if i > 0 && range.start_key < r.ranges[i - 1].end_key {
                Some("invalid key range, ranges overlapped")
            } else if !range.end_key.is_empty() && range.start_key > range.end_key {
                Some("invalid key range, start key > end key")
            } else {
                None
            };
            if let Some(message) = message {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message: message.into(),
                });
                return Ok(Response::new(response));
            }
        }
        let request_buckets = r.need_buckets;
        let ranges: Vec<_> = r
            .ranges
            .into_iter()
            .map(|r| KeyRange {
                start_key: r.start_key,
                end_key: r.end_key,
            })
            .collect();
        let cluster = self.cluster.lock().unwrap();
        match cluster.regions.batch_scan_regions(
            &ranges,
            BatchScanRegionsOptions {
                limit: r.limit as isize,
                output_must_contain_all_key_range: r.contain_all_key_range,
            },
        ) {
            Ok(regions) => {
                response.regions = regions
                    .into_iter()
                    .map(|r| {
                        let mut wire = cluster_read::region(&r);
                        if request_buckets {
                            wire.buckets =
                                cluster_read::point(self.header(), Some(&r), true).buckets;
                        }
                        wire
                    })
                    .collect()
            }
            Err(error) => {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::RegionsNotContainAllKeyRange as i32,
                    message: error.to_string(),
                })
            }
        }
        Ok(Response::new(response))
    }
    async fn query_region(
        &self,
        request: Request<tonic::Streaming<pdpb::QueryRegionRequest>>,
    ) -> Result<Response<tonic::codegen::BoxStream<pdpb::QueryRegionResponse>>, Status> {
        let service = self.clone();
        let stream = futures_util::stream::try_unfold(
            (request.into_inner(), service),
            |(mut input, service)| async move {
                let next = tokio::select! {biased; _=service.cancel.cancelled()=>return Err(Status::unavailable("server closed")), result=input.message()=>result?};
                let Some(r) = next else { return Ok(None) };
                let received = r.header.as_ref().map_or(0, |h| h.cluster_id);
                if received != service.cluster_id {
                    return Err(Status::failed_precondition(format!(
                        "mismatch cluster id, need {} but got {received}",
                        service.cluster_id
                    )));
                }
                let result = service.cluster.lock().unwrap().regions.query_regions(
                    &r.keys.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                    &r.prev_keys.iter().map(Vec::as_slice).collect::<Vec<_>>(),
                    &r.ids,
                    r.need_buckets,
                );
                let response = cluster_read::query(service.header(), result);
                Ok(Some((response, (input, service))))
            },
        );
        Ok(Response::new(Box::pin(stream)))
    }
}
