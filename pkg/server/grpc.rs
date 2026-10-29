// Copyright 2026 AsterSQL.
use crate::*;
use astersql_pd_proto::pdpb::{self, pd_server::Pd};
use std::pin::Pin;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

pub struct Grpc {
    pub services: Arc<dyn PdServices>,
    pub cancel: CancellationToken,
}
impl Grpc {
    pub(crate) fn header(&self) -> Option<pdpb::ResponseHeader> {
        Some(pdpb::ResponseHeader {
            cluster_id: self.services.identity().cluster_id,
            ..Default::default()
        })
    }
    pub(crate) async fn bootstrap_header(&self) -> Result<Option<pdpb::ResponseHeader>, Status> {
        let bootstrapped = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = self.services.is_bootstrapped() => result.map_err(Status::unavailable)?,
        };
        if bootstrapped {
            Ok(None)
        } else {
            Ok(Some(crate::request_role::not_bootstrapped(
                self.services.as_ref(),
            )))
        }
    }
    pub(crate) async fn check(&self, header: Option<&pdpb::RequestHeader>) -> Result<(), Status> {
        crate::request_role::validate(self.services.as_ref(), &self.cancel, header).await
    }
}
#[tonic::async_trait]
impl Pd for Grpc {
    async fn split_and_scatter_regions(
        &self,
        r: Request<pdpb::SplitAndScatterRegionsRequest>,
    ) -> Result<Response<pdpb::SplitAndScatterRegionsResponse>, Status> {
        self.split_scatter_request(r.into_inner()).await
    }

    async fn split_regions(
        &self,
        r: Request<pdpb::SplitRegionsRequest>,
    ) -> Result<Response<pdpb::SplitRegionsResponse>, Status> {
        self.split_regions_request(r.into_inner()).await
    }

    async fn scatter_region(
        &self,
        r: Request<pdpb::ScatterRegionRequest>,
    ) -> Result<Response<pdpb::ScatterRegionResponse>, Status> {
        self.scatter_request(r.into_inner()).await
    }
    async fn get_operator(
        &self,
        r: Request<pdpb::GetOperatorRequest>,
    ) -> Result<Response<pdpb::GetOperatorResponse>, Status> {
        self.operator_request(r.into_inner()).await
    }
    async fn get_cluster_config(
        &self,
        r: Request<pdpb::GetClusterConfigRequest>,
    ) -> Result<Response<pdpb::GetClusterConfigResponse>, Status> {
        self.get_cluster_config_request(r.into_inner()).await
    }
    async fn put_cluster_config(
        &self,
        r: Request<pdpb::PutClusterConfigRequest>,
    ) -> Result<Response<pdpb::PutClusterConfigResponse>, Status> {
        self.put_cluster_config_request(r.into_inner()).await
    }
    async fn bootstrap(
        &self,
        r: Request<pdpb::BootstrapRequest>,
    ) -> Result<Response<pdpb::BootstrapResponse>, Status> {
        self.bootstrap_request(r.into_inner())
            .await
            .map(Response::new)
    }
    async fn is_bootstrapped(
        &self,
        r: Request<pdpb::IsBootstrappedRequest>,
    ) -> Result<Response<pdpb::IsBootstrappedResponse>, Status> {
        self.bootstrapped_request(r.into_inner())
            .await
            .map(Response::new)
    }

    async fn put_store(
        &self,
        r: Request<pdpb::PutStoreRequest>,
    ) -> Result<Response<pdpb::PutStoreResponse>, Status> {
        self.put_store_request(r.into_inner()).await
    }
    async fn get_all_stores(
        &self,
        r: Request<pdpb::GetAllStoresRequest>,
    ) -> Result<Response<pdpb::GetAllStoresResponse>, Status> {
        self.all_stores_request(r.into_inner()).await
    }
    async fn get_store(
        &self,
        request: Request<pdpb::GetStoreRequest>,
    ) -> Result<Response<pdpb::GetStoreResponse>, Status> {
        let request = request.into_inner();
        self.check(request.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetStoreResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let service = self
            .services
            .cluster_read_service()
            .ok_or_else(|| Status::unimplemented("cluster reads are not configured"))?;
        let store = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = service.store(request.store_id) => result.map_err(Status::unavailable)?,
        };
        let mut header = self.header().unwrap();
        let (store, stats) = match store {
            Some(store) => (
                store
                    .meta()
                    .cloned()
                    .map(crate::core_store_adapter::store_to_proto),
                Some(crate::core_store_adapter::store_stats_to_proto(
                    store.store_stats(),
                )),
            ),
            None => {
                header.error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message: format!("invalid store ID {}, not found", request.store_id),
                });
                (None, None)
            }
        };
        Ok(Response::new(pdpb::GetStoreResponse {
            header: Some(header),
            store,
            stats,
        }))
    }
    async fn get_region(
        &self,
        request: Request<pdpb::GetRegionRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        self.region_request(request, false).await.map(Response::new)
    }
    async fn get_prev_region(
        &self,
        request: Request<pdpb::GetRegionRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        self.region_request(request, true).await.map(Response::new)
    }
    async fn query_region(
        &self,
        r: Request<tonic::Streaming<pdpb::QueryRegionRequest>>,
    ) -> Result<Response<crate::grpc_region_query::QueryStream>, Status> {
        Ok(self.query_stream(r))
    }
    async fn scan_regions(
        &self,
        r: Request<pdpb::ScanRegionsRequest>,
    ) -> Result<Response<pdpb::ScanRegionsResponse>, Status> {
        self.scan_regions_request(r).await.map(Response::new)
    }
    async fn batch_scan_regions(
        &self,
        r: Request<pdpb::BatchScanRegionsRequest>,
    ) -> Result<Response<pdpb::BatchScanRegionsResponse>, Status> {
        self.batch_scan_regions_request(r).await.map(Response::new)
    }
    async fn get_region_by_id(
        &self,
        request: Request<pdpb::GetRegionByIdRequest>,
    ) -> Result<Response<pdpb::GetRegionResponse>, Status> {
        let allow = request.metadata().contains_key("pd-allow-follower-handle");
        let r = request.into_inner();
        self.lookup_region(
            r.header.as_ref(),
            None,
            r.region_id,
            false,
            r.need_buckets,
            allow,
        )
        .await
        .map(Response::new)
    }

    async fn report_buckets(
        &self,
        request: Request<tonic::Streaming<pdpb::ReportBucketsRequest>>,
    ) -> Result<Response<pdpb::ReportBucketsResponse>, Status> {
        let mut input = request.into_inner();
        loop {
            let request = tokio::select! { biased;
                _ = self.cancel.cancelled() => return Err(Status::cancelled("bucket report stream cancelled")),
                request = input.message() => request?,
            };
            // Go sends no success response at EOF. Its client-streaming method
            // is observed by tonic clients as Unknown/EOF, not an empty ack.
            let request = request.ok_or_else(|| Status::unknown("EOF"))?;
            let process = async {
                let service = self.services.bucket_heartbeat_service().ok_or_else(|| {
                    Status::unimplemented("bucket heartbeat state service is not configured")
                })?;
                if !self
                    .services
                    .is_bootstrapped()
                    .await
                    .map_err(Status::unavailable)?
                    || !service.is_bootstrapped()
                {
                    let mut header = self.header().unwrap();
                    header.error = Some(pdpb::Error {
                        r#type: pdpb::ErrorType::NotBootstrapped as i32,
                        message: "cluster is not bootstrapped".into(),
                    });
                    return Ok(Some(pdpb::ReportBucketsResponse {
                        header: Some(header),
                    }));
                }
                self.check(request.header.as_ref()).await?;
                if let Some(buckets) = request.buckets.filter(|b| !b.keys.is_empty()) {
                    // Canonical host admission owns the applied flag and worker
                    // submission. Missing regions are ignored and reception continues.
                    let _ = service
                        .bucket_heartbeat(crate::bucket_heartbeat_adapter::bucket_from_proto(
                            buckets,
                        ))
                        .await;
                }
                Ok::<_, Status>(None)
            };
            let response = tokio::select! { biased;
                _ = self.cancel.cancelled() => return Err(Status::cancelled("bucket report stream cancelled")),
                response = process => response?,
            };
            if let Some(response) = response {
                return Ok(Response::new(response));
            }
        }
    }

    async fn region_heartbeat(
        &self,
        request: Request<tonic::Streaming<pdpb::RegionHeartbeatRequest>>,
    ) -> Result<
        Response<
            Pin<
                Box<
                    dyn Stream<Item = Result<pdpb::RegionHeartbeatResponse, Status>>
                        + Send
                        + 'static,
                >,
            >,
        >,
        Status,
    > {
        let input = request.into_inner();
        let services = self.services.clone();
        let cancel = self.cancel.child_token();
        let (commands_tx, commands_rx) = tokio::sync::mpsc::channel(1024);
        let commands_cancel = cancel.clone();
        // The output owns the receive loop, including the in-flight state future.
        // Accepted reports produce no ack; cancellation/drop detaches no worker.
        let output = futures_util::stream::unfold(
            (input, services, cancel, false, commands_tx),
            |(mut input, services, cancel, terminal, commands_tx)| async move {
                if terminal {
                    cancel.cancel();
                    return None;
                }
                loop {
                    let next = tokio::select! { biased;
                        _ = cancel.cancelled() => return None,
                        next = input.message() => next,
                    };
                    let req = match next {
                        Ok(Some(req)) => req,
                        Ok(None) => {
                            cancel.cancel();
                            return None;
                        }
                        Err(e) => {
                            return Some((Err(e), (input, services, cancel, true, commands_tx)));
                        }
                    };
                    let cluster_id = services.identity().cluster_id;
                    let result = async {
                        let service = services.region_heartbeat_service().ok_or_else(|| {
                            Status::unimplemented(
                                "region heartbeat state service is not configured",
                            )
                        })?;
                        let error_response =
                            |kind: pdpb::ErrorType, message: String, target_peer| {
                                pdpb::RegionHeartbeatResponse {
                                    header: Some(pdpb::ResponseHeader {
                                        cluster_id,
                                        error: Some(pdpb::Error {
                                            r#type: kind as i32,
                                            message,
                                        }),
                                    }),
                                    target_peer,
                                    ..Default::default()
                                }
                            };
                        if !services
                            .is_bootstrapped()
                            .await
                            .map_err(Status::unavailable)?
                            || !service.is_bootstrapped()
                        {
                            return Ok(Some((
                                error_response(
                                    pdpb::ErrorType::NotBootstrapped,
                                    "cluster is not bootstrapped".into(),
                                    None,
                                ),
                                true,
                            )));
                        }
                        crate::request_role::validate(
                            services.as_ref(),
                            &cancel,
                            req.header.as_ref(),
                        )
                        .await?;
                        let store_id = req.leader.as_ref().map(|p| p.store_id).unwrap_or(0);
                        match service.region_store_state(store_id) {
                            None => {
                                return Err(Status::unknown(format!(
                                    "invalid store ID {store_id}, not found"
                                )));
                            }
                            Some(astersql_pd_core::NodeState::Removed) => return Ok(None),
                            _ => {}
                        }
                        service
                            .bind_heartbeat(
                                store_id,
                                std::sync::Arc::new(
                                    crate::heartbeat_stream_adapter::CommandStream {
                                        sender: commands_tx.clone(),
                                        cancellation: cancel.clone(),
                                    },
                                ),
                            )
                            .await
                            .map_err(|e| Status::unavailable(e.0))?;
                        let target = req.leader.clone();
                        let region = match crate::core_region_adapter::region_from_heartbeat(
                            req,
                            service.flow_round_divisor(),
                        ) {
                            Ok(region) => region,
                            Err(message) => {
                                return Ok(Some((
                                    error_response(
                                        pdpb::ErrorType::Unknown,
                                        message.into(),
                                        target,
                                    ),
                                    false,
                                )));
                            }
                        };
                        let invalid = if region.leader().is_none() {
                            Some("invalid request leader")
                        } else if region.meta().id == 0 {
                            Some("invalid request region")
                        } else if region.meta().peers.is_empty() {
                            Some("invalid region, zero region peer count")
                        } else {
                            None
                        };
                        let error = match invalid {
                            Some(message) => Some(message.into()),
                            None => service.region_heartbeat(region).await.err().map(|e| e.0),
                        };
                        Ok(error.map(|message| {
                            (
                                error_response(pdpb::ErrorType::Unknown, message, target),
                                false,
                            )
                        }))
                    };
                    let result = tokio::select! { biased;
                        _ = cancel.cancelled() => return None,
                        result = result => result,
                    };
                    match result {
                        Ok(None) => continue,
                        Ok(Some((response, terminal))) => {
                            return Some((
                                Ok(response),
                                (input, services, cancel, terminal, commands_tx),
                            ));
                        }
                        Err(error) => {
                            return Some((
                                Err(error),
                                (input, services, cancel, true, commands_tx),
                            ));
                        }
                    }
                }
            },
        );
        let commands = futures_util::stream::unfold(
            (commands_rx, commands_cancel),
            |(mut rx, cancel)| async move {
                let command = tokio::select! { biased;
                    _ = cancel.cancelled() => return None,
                    command = rx.recv() => command?,
                };
                let response = match command {
                    astersql_pd_schedule_hbstream::stream_lifecycle::HeartbeatMessage::Command(command) =>
                        crate::schedule_wire::to_pdpb(&command).map_err(|e| Status::internal(e.to_string())),
                    astersql_pd_schedule_hbstream::stream_lifecycle::HeartbeatMessage::KeepAlive{cluster_id} =>
                        Ok(pdpb::RegionHeartbeatResponse{header:Some(pdpb::ResponseHeader{cluster_id,error:None}),..Default::default()}),
                };
                Some((response, (rx, cancel)))
            },
        );
        Ok(Response::new(Box::pin(futures_util::stream::select(
            output, commands,
        ))))
    }
    async fn store_heartbeat(
        &self,
        request: Request<pdpb::StoreHeartbeatRequest>,
    ) -> Result<Response<pdpb::StoreHeartbeatResponse>, Status> {
        use crate::store_heartbeat_adapter::StoreHeartbeatError;
        let request = request.into_inner();
        self.check(request.header.as_ref()).await?;
        self.services.healthy().await.map_err(Status::unavailable)?;
        // Do not include a debug dump of the wire payload in transport errors.
        let stats = request
            .stats
            .ok_or_else(|| Status::unknown("invalid store heartbeat command"))?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::StoreHeartbeatResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let service = self.services.store_heartbeat_service().ok_or_else(|| {
            Status::unimplemented("store heartbeat state service is not configured")
        })?;
        // These fields belong to recovery/replication controllers outside this
        // state adapter. In particular a recovery report must bypass statistics.
        if request.store_report.is_some() || request.dr_autosync_status.is_some() {
            return Err(Status::unimplemented(
                "store recovery and DR status are unsupported",
            ));
        }
        let stats = crate::core_store_adapter::store_stats_from_proto(Some(stats)).unwrap();
        let result = service.store_heartbeat(stats).await;
        let mut header = self.header().unwrap();
        let state = match result {
            Ok(state) => match state {
                astersql_pd_core::NodeState::Preparing => 0,
                astersql_pd_core::NodeState::Serving => 1,
                astersql_pd_core::NodeState::Removing => 2,
                astersql_pd_core::NodeState::Removed => 3,
                astersql_pd_core::NodeState::Unknown(value) => value,
            },
            Err(error) => {
                let (kind, message) = match error {
                    StoreHeartbeatError::NotBootstrapped => (
                        pdpb::ErrorType::NotBootstrapped,
                        "cluster is not bootstrapped".into(),
                    ),
                    StoreHeartbeatError::StoreNotFound(id) => {
                        (pdpb::ErrorType::Unknown, format!("store {id} not found"))
                    }
                    StoreHeartbeatError::Removed => {
                        (pdpb::ErrorType::StoreTombstone, "store is tombstone".into())
                    }
                    StoreHeartbeatError::Other(message) => (pdpb::ErrorType::Unknown, message),
                };
                header.error = Some(pdpb::Error {
                    r#type: kind as i32,
                    message,
                });
                0
            }
        };
        Ok(Response::new(pdpb::StoreHeartbeatResponse {
            header: Some(header),
            state,
            ..Default::default()
        }))
    }
    async fn get_members(
        &self,
        _: Request<pdpb::GetMembersRequest>,
    ) -> Result<Response<pdpb::GetMembersResponse>, Status> {
        crate::grpc_members::members(self.services.as_ref(), &self.cancel)
            .await
            .map(Response::new)
    }
    async fn get_cluster_info(
        &self,
        _: Request<pdpb::GetClusterInfoRequest>,
    ) -> Result<Response<pdpb::GetClusterInfoResponse>, Status> {
        if self.cancel.is_cancelled() {
            return Err(Status::unavailable("server not started"));
        }
        Ok(Response::new(pdpb::GetClusterInfoResponse {
            header: self.header(),
            service_modes: vec![pdpb::ServiceMode::PdSvcMode as i32],
            tso_urls: vec![],
        }))
    }

    async fn report_split(
        &self,
        r: Request<pdpb::ReportSplitRequest>,
    ) -> Result<Response<pdpb::ReportSplitResponse>, Status> {
        let r = r.into_inner();
        Ok(Response::new(pdpb::ReportSplitResponse {
            header: self
                .report_split_header(r.header.as_ref(), &[r.left, r.right])
                .await?,
        }))
    }
    async fn report_batch_split(
        &self,
        r: Request<pdpb::ReportBatchSplitRequest>,
    ) -> Result<Response<pdpb::ReportBatchSplitResponse>, Status> {
        let r = r.into_inner();
        Ok(Response::new(pdpb::ReportBatchSplitResponse {
            header: self
                .report_split_header(
                    r.header.as_ref(),
                    &r.regions.into_iter().map(Some).collect::<Vec<_>>(),
                )
                .await?,
        }))
    }
    async fn ask_split(
        &self,
        r: Request<pdpb::AskSplitRequest>,
    ) -> Result<Response<pdpb::AskSplitResponse>, Status> {
        self.ask_split_request(r.into_inner()).await
    }
    async fn ask_batch_split(
        &self,
        r: Request<pdpb::AskBatchSplitRequest>,
    ) -> Result<Response<pdpb::AskBatchSplitResponse>, Status> {
        self.ask_batch_split_request(r.into_inner()).await
    }
    async fn alloc_id(
        &self,
        req: Request<pdpb::AllocIdRequest>,
    ) -> Result<Response<pdpb::AllocIdResponse>, Status> {
        crate::grpc_alloc_id::handle(self.services.as_ref(), &self.cancel, req).await
    }

    async fn tso(
        &self,
        req: Request<tonic::Streaming<pdpb::TsoRequest>>,
    ) -> Result<
        Response<Pin<Box<dyn Stream<Item = Result<pdpb::TsoResponse, Status>> + Send + 'static>>>,
        Status,
    > {
        let input = req.into_inner();
        let services = self.services.clone();
        let cancel = self.cancel.clone();
        // The response stream owns all work; dropping it cannot detach a request worker.
        let output = futures_util::stream::unfold(
            (input, services, cancel, false),
            |(mut input, services, cancel, terminal)| async move {
                if terminal {
                    return None;
                }
                let request = tokio::select! { biased;
                    _ = cancel.cancelled() => return None,
                    request = input.message() => request,
                };
                let result = match request {
                    Ok(Some(req)) => {
                        if req.header.as_ref().map(|h| h.cluster_id).unwrap_or(0)
                            != services.identity().cluster_id
                        {
                            Err(Status::failed_precondition(format!(
                                "mismatch cluster id, need {} but got {}",
                                services.identity().cluster_id,
                                req.header.as_ref().map(|h| h.cluster_id).unwrap_or(0)
                            )))
                        } else if !req.dc_location.is_empty() && req.dc_location != "global" {
                            Err(Status::unimplemented("local TSO is unsupported"))
                        } else if req.count == 0 || req.count >= astersql_pd_tso::MAX_LOGICAL {
                            Err(Status::unknown(
                                "[PD:tso:ErrGenerateTimestamp]generate timestamp failed, tso count should be positive",
                            ))
                        } else {
                            services
                                .timestamp(req.count)
                                .await
                                .map(|ts| pdpb::TsoResponse {
                                    header: Some(pdpb::ResponseHeader {
                                        cluster_id: services.identity().cluster_id,
                                        ..Default::default()
                                    }),
                                    count: req.count,
                                    timestamp: Some(pdpb::Timestamp {
                                        physical: ts.physical as i64,
                                        logical: ts.logical as i64,
                                        suffix_bits: 0,
                                    }),
                                })
                                .map_err(Status::unavailable)
                        }
                    }
                    Ok(None) => return None,
                    Err(status) => Err(status),
                };
                let terminal = result.is_err();
                Some((result, (input, services, cancel, terminal)))
            },
        );
        Ok(Response::new(Box::pin(output)))
    }
}
