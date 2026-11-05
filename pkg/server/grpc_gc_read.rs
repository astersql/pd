// Copyright 2026 AsterSQL.
//! State query adapters use the host's canonical GC manager.
use crate::{
    grpc::Grpc,
    grpc_gc_barrier::{global_info, info, now},
};
use astersql_pd_proto::pdpb;
use astersql_pd_storage_domain::BarrierTime;
use tonic::{Response, Status};
fn state(s: astersql_pd_gc::GCState, now: BarrierTime) -> pdpb::GcState {
    pdpb::GcState {
        keyspace_scope: Some(pdpb::KeyspaceScope {
            keyspace: Some(pdpb::keyspace_scope::Keyspace::KeyspaceId(s.keyspace_id)),
        }),
        is_keyspace_level_gc: s.is_keyspace_level,
        txn_safe_point: s.txn_safe_point,
        gc_safe_point: s.gc_safe_point,
        gc_barriers: s.gc_barriers.into_iter().map(|b| info(b, now)).collect(),
    }
}
impl Grpc {
    pub(crate) async fn get_gc_state_request(
        &self,
        r: pdpb::GetGcStateRequest,
    ) -> Result<Response<pdpb::GetGcStateResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetGcStateResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=async {
                let m=self.services.gc_manager().await?;
                let id=crate::grpc_gc_advance::scope(r.keyspace_scope);
                if r.include_global_gc_barriers {m.get_gc_state_with_global_barriers(id,r.exclude_gc_barriers).await.map_err(|e|e.to_string())}
                else {m.get_gc_state(id,r.exclude_gc_barriers).await.map(|s|(s,vec![])).map_err(|e|e.to_string())}
            }=>result,
        };
        let mut response = pdpb::GetGcStateResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok((s, b)) => {
                let now = now()?;
                response.gc_state = Some(state(s, now));
                if r.include_global_gc_barriers {
                    response.global_gc_barriers = Some(pdpb::GlobalGcBarriersInfo {
                        barriers: b.into_iter().map(|b| global_info(b, now)).collect(),
                    });
                }
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
    pub(crate) async fn get_all_gc_states_request(
        &self,
        r: pdpb::GetAllKeyspacesGcStatesRequest,
    ) -> Result<Response<pdpb::GetAllKeyspacesGcStatesResponse>, Status> {
        self.check(r.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetAllKeyspacesGcStatesResponse {
                header: Some(header),
                ..Default::default()
            }));
        }
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=async {
                let m=self.services.gc_manager().await?;
                let states=m.get_all_keyspaces_gc_states(r.exclude_gc_barriers).await.map_err(|e|e.to_string())?;
                let barriers=if r.exclude_global_gc_barriers {vec![]}else{m.load_all_global_gc_barriers().await.map_err(|e|e.to_string())?};
                Ok::<_,String>((states,barriers))
            }=>result,
        };
        let mut response = pdpb::GetAllKeyspacesGcStatesResponse {
            header: self.header(),
            ..Default::default()
        };
        match result {
            Ok((s, b)) => {
                let now = now()?;
                response.gc_states = s.into_values().map(|s| state(s, now)).collect();
                response.global_gc_barriers = b.into_iter().map(|b| global_info(b, now)).collect();
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
