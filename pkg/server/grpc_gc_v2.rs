// Copyright 2026 AsterSQL.
//! Keyspace GC wire conversion delegates every decision to the term-owned manager.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use astersql_pd_storage_domain::BarrierTime;
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn get_all_gc_safe_point_v2_request(
        &self,
        r: pdpb::GetAllGcSafePointV2Request,
    ) -> Result<Response<pdpb::GetAllGcSafePointV2Response>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetAllGcSafePointV2Response {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=async{self.services.gc_manager().await?.get_all_keyspaces_gc_states(true).await.map_err(|e|e.to_string())}=>result,
        };
        let mut header = self.header();
        let gc_safe_points = match result {
            Ok(states) => states
                .into_values()
                .filter(|s| s.keyspace_id != astersql_pd_keypath::NULL_KEYSPACE_ID)
                .map(|s| pdpb::GcSafePointV2 {
                    keyspace: Some(pdpb::gc_safe_point_v2::Keyspace::KeyspaceId(s.keyspace_id)),
                    gc_safe_point: s.gc_safe_point,
                })
                .collect(),
            Err(message) => {
                header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message,
                });
                vec![]
            }
        };
        Ok(Response::new(pdpb::GetAllGcSafePointV2Response {
            header,
            gc_safe_points,
            revision: 0,
        }))
    }
    pub(crate) async fn get_gc_safe_point_v2_request(
        &self,
        r: pdpb::GetGcSafePointV2Request,
    ) -> Result<Response<pdpb::GetGcSafePointV2Response>, Status> {
        let keyspace_id = match r.keyspace {
            Some(pdpb::get_gc_safe_point_v2_request::Keyspace::KeyspaceId(id)) => id,
            _ => 0,
        };
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetGcSafePointV2Response {
                header: Some(header),
                ..Default::default()
            }));
        }
        let safe_point = tokio::select! {biased;
         _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
         result=async {self.services.gc_manager().await?.compatible_load_gc_safe_point(keyspace_id).await.map_err(|e|e.to_string())}=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::GetGcSafePointV2Response {
            header: self.header(),
            safe_point,
        }))
    }
    pub(crate) async fn update_gc_safe_point_v2_request(
        &self,
        r: pdpb::UpdateGcSafePointV2Request,
    ) -> Result<Response<pdpb::UpdateGcSafePointV2Response>, Status> {
        let keyspace_id = match r.keyspace {
            Some(pdpb::update_gc_safe_point_v2_request::Keyspace::KeyspaceId(id)) => id,
            _ => 0,
        };
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::UpdateGcSafePointV2Response {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
         _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
         result=async {self.services.gc_manager().await?.compatible_update_gc_safe_point(keyspace_id,r.safe_point).await.map_err(|e|e.to_string())}=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::UpdateGcSafePointV2Response {
            header: self.header(),
            new_safe_point: result.new_gc_safe_point,
        }))
    }
    pub(crate) async fn update_service_safe_point_v2_request(
        &self,
        r: pdpb::UpdateServiceSafePointV2Request,
    ) -> Result<Response<pdpb::UpdateServiceSafePointV2Response>, Status> {
        let keyspace_id = match r.keyspace {
            Some(pdpb::update_service_safe_point_v2_request::Keyspace::KeyspaceId(id)) => id,
            _ => 0,
        };
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::UpdateServiceSafePointV2Response {
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
          let result=manager.compatible_update_service_gc_safe_point(keyspace_id,&r.service_id,r.safe_point,r.ttl,now).await.map_err(|e|e.to_string())?;
          Ok::<_,String>((result,seconds))
         }=>result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::UpdateServiceSafePointV2Response {
            header: self.header(),
            service_id: result.minimum.service_id,
            ttl: result.minimum.expired_at - seconds,
            min_safe_point: result.minimum.safe_point,
        }))
    }
}
