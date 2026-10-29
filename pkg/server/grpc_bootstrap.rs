// Copyright 2026 AsterSQL.
//! Classic Bootstrap validation; transport values stop at this adapter.
use astersql_pd_proto::{metapb, pdpb};
use tonic::Status;

fn validate(
    request: &pdpb::BootstrapRequest,
    id: u64,
) -> Result<(&metapb::Store, &metapb::Region), String> {
    let store = request
        .store
        .as_ref()
        .ok_or_else(|| format!("missing store meta for bootstrap {id}"))?;
    if store.id == 0 {
        return Err("invalid zero store id".into());
    }
    let region = request
        .region
        .as_ref()
        .ok_or_else(|| format!("missing region meta for bootstrap {id}"))?;
    if !region.start_key.is_empty() || !region.end_key.is_empty() {
        return Err(format!(
            "invalid first region key range, must all be empty for bootstrap {id}"
        ));
    }
    if region.id == 0 {
        return Err("invalid zero region id".into());
    }
    if region.peers.len() != 1 {
        return Err(format!(
            "invalid first region peer count {}, must be 1 for bootstrap {id}",
            region.peers.len()
        ));
    }
    let peer = &region.peers[0];
    if peer.store_id != store.id {
        return Err(format!(
            "invalid peer store id {} != {} for bootstrap {id}",
            peer.store_id, store.id
        ));
    }
    if peer.id == 0 {
        return Err("invalid zero peer id".into());
    }
    Ok((store, region))
}
impl crate::grpc::Grpc {
    pub(crate) async fn bootstrap_request(
        &self,
        request: pdpb::BootstrapRequest,
    ) -> Result<pdpb::BootstrapResponse, Status> {
        crate::request_role::validate(
            self.services.as_ref(),
            &self.cancel,
            request.header.as_ref(),
        )
        .await?;
        let cluster_id = self.services.identity().cluster_id;
        let result = async {
            if self.services.is_bootstrapped().await? {
                return Ok(Some(pdpb::Error {
                    r#type: pdpb::ErrorType::AlreadyBootstrapped as i32,
                    message: "cluster is already bootstrapped".into(),
                }));
            }
            let (store, region) = validate(&request, cluster_id)?;
            let store = crate::core_store_adapter::store_from_proto(Some(store.clone())).unwrap();
            let region =
                crate::core_region_adapter::region_from_proto(Some(region.clone())).unwrap();
            if !self.services.bootstrap(store, region).await? {
                return Err("[PD:etcd:ErrEtcdTxnConflict]etcd transaction failed, conflicted and rolled back".into());
            }
            Ok(None)
        };
        let error = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = result => result.unwrap_or_else(|message|Some(pdpb::Error{r#type:pdpb::ErrorType::Unknown as i32,message})),
        };
        let replication_status =
            error
                .is_none()
                .then_some(astersql_pd_proto::replication_modepb::ReplicationStatus {
                    mode: astersql_pd_proto::replication_modepb::ReplicationMode::Majority as i32,
                    dr_auto_sync: None,
                });
        Ok(pdpb::BootstrapResponse {
            header: Some(pdpb::ResponseHeader { cluster_id, error }),
            replication_status,
            ..Default::default()
        })
    }
    pub(crate) async fn bootstrapped_request(
        &self,
        request: pdpb::IsBootstrappedRequest,
    ) -> Result<pdpb::IsBootstrappedResponse, Status> {
        crate::request_role::validate(
            self.services.as_ref(),
            &self.cancel,
            request.header.as_ref(),
        )
        .await?;
        let bootstrapped = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = self.services.is_bootstrapped() => result.map_err(Status::unavailable)?,
        };
        Ok(pdpb::IsBootstrappedResponse {
            header: Some(pdpb::ResponseHeader {
                cluster_id: self.services.identity().cluster_id,
                error: None,
            }),
            bootstrapped,
        })
    }
}
