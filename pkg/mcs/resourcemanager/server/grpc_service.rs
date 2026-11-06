// Copyright 2026 AsterSQL.
use crate::{Manager, grpc_codec as codec};
use astersql_pd_proto::resource_manager::{self as p, resource_manager_server::ResourceManager};
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio_util::sync::CancellationToken;
use tonic::{Request, Response, Status};
/// The embedding host owns serving state, term identity, storage, and cancellation.
#[async_trait::async_trait]
pub trait Host: Send + Sync {
    async fn manager(&self) -> Result<Arc<Mutex<Manager>>, Status>;
    fn reject_metadata_writes(&self) -> bool {
        false
    }
    async fn consumption_keyspace_exists(&self, id: u32) -> bool;
}
#[derive(Clone)]
pub struct Service {
    host: Arc<dyn Host>,
    cancel: CancellationToken,
}
impl Service {
    pub fn new(host: Arc<dyn Host>, cancel: CancellationToken) -> Self {
        Self { host, cancel }
    }
    async fn manager(&self) -> Result<OwnedMutexGuard<Manager>, Status> {
        if self.cancel.is_cancelled() {
            return Err(Status::unavailable("not leader"));
        }
        let manager = self.host.manager().await?;
        Ok(manager.lock_owned().await)
    }
}
#[tonic::async_trait]
impl ResourceManager for Service {
    async fn list_resource_groups(
        &self,
        req: Request<p::ListResourceGroupsRequest>,
    ) -> Result<Response<p::ListResourceGroupsResponse>, Status> {
        let req = req.into_inner();
        let id = codec::keyspace(req.keyspace_id.as_ref());
        let mut state = self.manager().await?;
        let groups = state
            .list(id, req.with_ru_stats)
            .await
            .map_err(Status::unknown)?;
        Ok(Response::new(p::ListResourceGroupsResponse {
            groups: groups.iter().map(|g| codec::encode(g, id)).collect(),
            ..Default::default()
        }))
    }
    async fn get_resource_group(
        &self,
        req: Request<p::GetResourceGroupRequest>,
    ) -> Result<Response<p::GetResourceGroupResponse>, Status> {
        let req = req.into_inner();
        let id = codec::keyspace(req.keyspace_id.as_ref());
        let mut state = self.manager().await?;
        let group = state
            .get(id, &req.resource_group_name, req.with_ru_stats)
            .await
            .map_err(Status::unknown)?;
        Ok(Response::new(p::GetResourceGroupResponse {
            group: Some(codec::encode(&group, id)),
            ..Default::default()
        }))
    }
    async fn add_resource_group(
        &self,
        req: Request<p::PutResourceGroupRequest>,
    ) -> Result<Response<p::PutResourceGroupResponse>, Status> {
        let mut state = self.manager().await?;
        if self.host.reject_metadata_writes() {
            return Err(Status::failed_precondition(
                "resource group metadata writes must be handled by PD",
            ));
        }
        let group = req.into_inner().group.unwrap_or_default();
        let id = codec::keyspace(group.keyspace_id.as_ref());
        state
            .add(id, codec::decode(group))
            .await
            .map_err(Status::unknown)?;
        Ok(Response::new(p::PutResourceGroupResponse {
            body: "Success!".into(),
            ..Default::default()
        }))
    }
    async fn modify_resource_group(
        &self,
        req: Request<p::PutResourceGroupRequest>,
    ) -> Result<Response<p::PutResourceGroupResponse>, Status> {
        let mut state = self.manager().await?;
        if self.host.reject_metadata_writes() {
            return Err(Status::failed_precondition(
                "resource group metadata writes must be handled by PD",
            ));
        }
        let group = req.into_inner().group.unwrap_or_default();
        let id = codec::keyspace(group.keyspace_id.as_ref());
        state
            .modify(id, codec::decode(group))
            .await
            .map_err(Status::unknown)?;
        Ok(Response::new(p::PutResourceGroupResponse {
            body: "Success!".into(),
            ..Default::default()
        }))
    }
    async fn delete_resource_group(
        &self,
        req: Request<p::DeleteResourceGroupRequest>,
    ) -> Result<Response<p::DeleteResourceGroupResponse>, Status> {
        let req = req.into_inner();
        let mut state = self.manager().await?;
        if self.host.reject_metadata_writes() {
            return Err(Status::failed_precondition(
                "resource group metadata writes must be handled by PD",
            ));
        }
        state
            .delete(
                codec::keyspace(req.keyspace_id.as_ref()),
                &req.resource_group_name,
            )
            .await
            .map_err(Status::unknown)?;
        Ok(Response::new(p::DeleteResourceGroupResponse {
            body: "Success!".into(),
            ..Default::default()
        }))
    }
    async fn acquire_token_buckets(
        &self,
        request: Request<tonic::Streaming<p::TokenBucketsRequest>>,
    ) -> Result<Response<tonic::codegen::BoxStream<p::TokenBucketsResponse>>, Status> {
        use futures_util::{StreamExt, stream};
        let input = request.into_inner();
        let service = self.clone();
        let output = stream::try_unfold((service, input), |(service, mut input)| async move {
            let request = tokio::select! {
                _=service.cancel.cancelled()=>return Err(Status::unknown("server closed")),
                request=input.message()=>request?,
            };
            let Some(request) = request else {
                return Ok(None);
            };
            let response = service.tokens(request).await?;
            Ok(Some((response, (service, input))))
        });
        let mut output: tonic::codegen::BoxStream<p::TokenBucketsResponse> = Box::pin(output);
        // Go commits stream headers on its first Send, after receiving the first request.
        let first = output.next().await.transpose()?;
        Ok(Response::new(Box::pin(
            stream::iter(first.map(Ok)).chain(output),
        )))
    }
}
impl Service {
    async fn tokens(
        &self,
        request: p::TokenBucketsRequest,
    ) -> Result<p::TokenBucketsResponse, Status> {
        let mut state = self.manager().await?;
        let manager = &mut *state;
        let mut response = p::TokenBucketsResponse::default();
        for request_item in request.requests {
            let id = codec::keyspace(request_item.keyspace_id.as_ref());
            let name = &request_item.resource_group_name;
            if manager.get(id, name, false).await.is_err() {
                continue;
            }
            if request_item.is_background && request_item.is_tiflash {
                return Err(Status::unknown(
                    "background and tiflash cannot be true at the same time",
                ));
            }
            if let Some(delta) = request_item.consumption_since_last_request {
                if id == u32::MAX
                    || manager.consumption_keyspaces.contains(&id)
                    || self.host.consumption_keyspace_exists(id).await
                {
                    manager.consumption_keyspaces.insert(id);
                    manager.record_consumption(id, name, &delta.into());
                }
            }
            if request_item.is_background {
                continue;
            }
            let mut item = p::TokenBucketResponse {
                resource_group_name: name.clone(),
                keyspace_id: codec::keyspace_value(id),
                ..Default::default()
            };
            if let Some(p::token_bucket_request::Request::RuItems(items)) = request_item.request {
                let mut previous = None;
                for ru in items.request_r_u {
                    if ru.r#type == 0 {
                        let now = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_err(|e| Status::unknown(e.to_string()))?
                            .as_nanos() as i64;
                        previous = manager
                            .request_ru(
                                id,
                                name,
                                now,
                                ru.value,
                                request.target_request_period_ms,
                                request.client_unique_id,
                            )
                            .await
                            .map(|(bucket, trickle)| p::GrantedRuTokenBucket {
                                granted_tokens: Some(bucket.into()),
                                trickle_time_ms: trickle,
                                ..Default::default()
                            });
                    }
                    if let Some(grant) = &previous {
                        item.granted_r_u_tokens.push(grant.clone());
                    }
                }
            }
            response.responses.push(item);
        }
        Ok(response)
    }
}
