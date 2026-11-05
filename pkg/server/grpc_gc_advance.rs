// Copyright 2026 AsterSQL.
//! Modern advancement RPCs use the canonical term-owned GC manager.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use astersql_pd_storage_domain::BarrierTime;
use tonic::{Response, Status};
pub(crate) fn scope(s: Option<pdpb::KeyspaceScope>) -> u32 {
    match s {
        None => astersql_pd_keypath::NULL_KEYSPACE_ID,
        Some(s) => match s.keyspace {
            Some(pdpb::keyspace_scope::Keyspace::KeyspaceId(id)) => id,
            _ => 0,
        },
    }
}
impl Grpc {
    pub(crate) async fn advance_gc_safe_point_request(
        &self,
        r: pdpb::AdvanceGcSafePointRequest,
    ) -> Result<Response<pdpb::AdvanceGcSafePointResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::AdvanceGcSafePointResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=async {self.services.gc_manager().await?.advance_gc_safe_point(scope(r.keyspace_scope),r.target).await.map_err(|e|e.to_string())}=>result,
        };
        let mut response = pdpb::AdvanceGcSafePointResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok(r) => {
                response.old_gc_safe_point = r.old_gc_safe_point;
                response.new_gc_safe_point = r.new_gc_safe_point;
            }
            Err(message) => {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message,
                })
            }
        }
        Ok(Response::new(response))
    }
    pub(crate) async fn advance_txn_safe_point_request(
        &self,
        r: pdpb::AdvanceTxnSafePointRequest,
    ) -> Result<Response<pdpb::AdvanceTxnSafePointResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::AdvanceTxnSafePointResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=async {
                let manager=self.services.gc_manager().await?;
                let now=std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?;
                manager.advance_txn_safe_point(scope(r.keyspace_scope),r.target,BarrierTime::from_unix(now.as_secs() as i64,now.subsec_nanos() as i64)).await.map_err(|e|e.to_string())
            }=>result,
        };
        let mut response = pdpb::AdvanceTxnSafePointResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok(r) => {
                response.old_txn_safe_point = r.old_txn_safe_point;
                response.new_txn_safe_point = r.new_txn_safe_point;
                response.blocker_description = r.blocker_description;
            }
            Err(message) => {
                response.header.as_mut().unwrap().error = Some(pdpb::Error {
                    r#type: pdpb::ErrorType::Unknown as i32,
                    message,
                })
            }
        }
        Ok(Response::new(response))
    }
}
