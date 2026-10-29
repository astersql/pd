// Copyright 2026 AsterSQL.
//! Classic cluster metadata RPCs use the canonical persistence codec and term fence.
use crate::grpc::Grpc;
use astersql_pd_keypath::ClusterPaths;
use astersql_pd_proto::{metapb, pdpb};
use astersql_pd_storage_endpoint::{Cluster, StorageEndpoint};
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn get_cluster_config_request(
        &self,
        request: pdpb::GetClusterConfigRequest,
    ) -> Result<Response<pdpb::GetClusterConfigResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::GetClusterConfigResponse {
                header: Some(header),
                cluster: None,
            }));
        }
        let endpoint = StorageEndpoint::new(
            self.services
                .cluster_config_store()
                .ok_or_else(|| Status::unavailable("server not started"))?,
        );
        let paths = ClusterPaths::new(self.services.identity().cluster_id);
        let meta = tokio::select! { biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=endpoint.load_meta(&paths)=>result.map_err(|error|Status::unknown(error.to_string()))?,
        };
        Ok(Response::new(pdpb::GetClusterConfigResponse {
            header: self.header(),
            cluster: meta.map(|meta| metapb::Cluster {
                id: meta.id,
                max_peer_count: meta.max_peer_count,
            }),
        }))
    }
    pub(crate) async fn put_cluster_config_request(
        &self,
        request: pdpb::PutClusterConfigRequest,
    ) -> Result<Response<pdpb::PutClusterConfigResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Response::new(pdpb::PutClusterConfigResponse {
                header: Some(header),
            }));
        }
        let id = self.services.identity().cluster_id;
        let result = async {
            let meta = request.cluster.as_ref();
            if meta.map_or(0, |meta| meta.id) != id {
                let text = match meta {
                    None => "<nil>".to_string(),
                    Some(meta) => format!(
                        "{}{}",
                        if meta.id == 0 {
                            String::new()
                        } else {
                            format!("id:{} ", meta.id)
                        },
                        if meta.max_peer_count == 0 {
                            String::new()
                        } else {
                            format!("max_peer_count:{} ", meta.max_peer_count)
                        }
                    ),
                };
                return Err(format!("invalid cluster {text}, mismatch cluster id {id}"));
            }
            let meta = meta.ok_or("missing cluster metadata")?;
            let endpoint = StorageEndpoint::new(
                self.services
                    .cluster_config_store()
                    .ok_or("server not started")?,
            );
            endpoint
                .save_meta(
                    &ClusterPaths::new(id),
                    &Cluster {
                        id: meta.id,
                        max_peer_count: meta.max_peer_count,
                    },
                )
                .await
                .map_err(|error| error.to_string())
        };
        let result = tokio::select! {biased;
            _=self.cancel.cancelled()=>return Err(Status::unavailable("server not started")),
            result=result=>result,
        };
        let mut header = self.header().unwrap();
        header.error = result.err().map(|message| pdpb::Error {
            r#type: pdpb::ErrorType::Unknown as i32,
            message,
        });
        Ok(Response::new(pdpb::PutClusterConfigResponse {
            header: Some(header),
        }))
    }
}
