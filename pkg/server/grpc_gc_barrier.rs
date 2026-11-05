// Copyright 2026 AsterSQL.
//! Barrier RPC adapters for the canonical term-owned GC manager.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use astersql_pd_storage_domain::{BarrierTime, GCBarrier};
use astersql_pd_typeutil::{Duration, saturating_std_duration_from_seconds};
use tonic::{Response, Status};
fn now() -> Result<BarrierTime, Status> {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| Status::internal(e.to_string()))?;
    Ok(BarrierTime::from_unix(
        n.as_secs() as i64,
        n.subsec_nanos() as i64,
    ))
}
fn info(b: GCBarrier, now: BarrierTime) -> pdpb::GcBarrierInfo {
    pdpb::GcBarrierInfo {
        barrier_id: b.barrier_id,
        barrier_ts: b.barrier_ts,
        ttl_seconds: b.expiration_time.map_or(i64::MAX, |e| {
            e.unix_seconds()
                .saturating_sub(now.unix_seconds())
                .saturating_sub(i64::from(
                    now != BarrierTime::from_unix(now.unix_seconds(), 0),
                ))
                .max(0)
        }),
    }
}
impl Grpc {
    pub(crate) async fn set_gc_barrier_request(
        &self,
        r: pdpb::SetGcBarrierRequest,
    ) -> Result<Response<pdpb::SetGcBarrierResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::SetGcBarrierResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let now = now()?;
        let ttl = Duration::from_nanos(
            saturating_std_duration_from_seconds(r.ttl_seconds).as_nanos() as i64,
        );
        let result = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = async {
                self.services.gc_manager().await?
                    .set_gc_barrier(super::grpc_gc_advance::scope(r.keyspace_scope), &r.barrier_id, r.barrier_ts, ttl, now)
                    .await.map_err(|e| e.to_string())
            } => result,
        };
        let mut response = pdpb::SetGcBarrierResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok(b) => response.new_barrier_info = Some(info(b, now)),
            Err(message) => {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message,
                })
            }
        };
        Ok(Response::new(response))
    }
    pub(crate) async fn delete_gc_barrier_request(
        &self,
        r: pdpb::DeleteGcBarrierRequest,
    ) -> Result<Response<pdpb::DeleteGcBarrierResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::DeleteGcBarrierResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let now = now()?;
        let result = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = async {
                self.services.gc_manager().await?
                    .delete_gc_barrier(super::grpc_gc_advance::scope(r.keyspace_scope), &r.barrier_id)
                    .await.map_err(|e| e.to_string())
            } => result,
        };
        let mut response = pdpb::DeleteGcBarrierResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok(b) => response.deleted_barrier_info = b.map(|b| info(b, now)),
            Err(message) => {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message,
                })
            }
        };
        Ok(Response::new(response))
    }
}
