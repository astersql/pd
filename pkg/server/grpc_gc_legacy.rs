// Copyright 2026 AsterSQL.
//! Legacy wire conversion delegates every GC decision to the term-owned manager.
use crate::grpc::Grpc;
use astersql_pd_keypath::NULL_KEYSPACE_ID;
use astersql_pd_proto::pdpb;
use astersql_pd_storage_domain::BarrierTime;
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn get_gc_safe_point_request(
        &self,
        r: pdpb::GetGcSafePointRequest,
    ) -> Result<Response<pdpb::GetGcSafePointResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetGcSafePointResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let safe_point = tokio::select! {biased;
         _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
         result=async {self.services.gc_manager().await?.compatible_load_gc_safe_point(NULL_KEYSPACE_ID).await.map_err(|e|e.to_string())}=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::GetGcSafePointResponse {
            header: self.header(),
            safe_point,
        }))
    }
    pub(crate) async fn update_gc_safe_point_request(
        &self,
        r: pdpb::UpdateGcSafePointRequest,
    ) -> Result<Response<pdpb::UpdateGcSafePointResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::UpdateGcSafePointResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
         _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
         result=async {self.services.gc_manager().await?.compatible_update_gc_safe_point(NULL_KEYSPACE_ID,r.safe_point).await.map_err(|e|e.to_string())}=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::UpdateGcSafePointResponse {
            header: self.header(),
            new_safe_point: result.new_gc_safe_point,
        }))
    }
    pub(crate) async fn update_service_gc_safe_point_request(
        &self,
        r: pdpb::UpdateServiceGcSafePointRequest,
    ) -> Result<Response<pdpb::UpdateServiceGcSafePointResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::UpdateServiceGcSafePointResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let (result, seconds) = tokio::select! {biased;
         _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
         result=async {
          let manager=self.services.gc_manager().await?;
          let timestamp=self.services.timestamp(1).await?;
          let seconds=(timestamp.physical/1000)as i64;
          let now=BarrierTime::from_unix(seconds,((timestamp.physical%1000)*1_000_000)as i64);
          let result=manager.compatible_update_service_gc_safe_point(NULL_KEYSPACE_ID,&r.service_id,r.safe_point,r.ttl,now).await.map_err(|e|e.to_string())?;
          Ok::<_,String>((result,seconds))
         }=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::UpdateServiceGcSafePointResponse {
            header: self.header(),
            service_id: result.minimum.service_id,
            ttl: result.minimum.expired_at - seconds,
            min_safe_point: result.minimum.safe_point,
        }))
    }
}
