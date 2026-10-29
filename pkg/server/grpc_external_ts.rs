// Copyright 2026 AsterSQL.
//! External timestamps share the term-owned host and existing hexadecimal storage.
use crate::grpc::Grpc;
use astersql_pd_proto::pdpb;
use tonic::{Response, Status};
impl Grpc {
    pub(crate) async fn get_external_timestamp_request(
        &self,
        request: pdpb::GetExternalTimestampRequest,
    ) -> Result<Response<pdpb::GetExternalTimestampResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        let timestamp = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = async {
                if !self.services.is_bootstrapped().await? { return Ok(0); }
                self.services.external_timestamp().await
            } => result.map_err(Status::unknown)?,
        };
        Ok(Response::new(pdpb::GetExternalTimestampResponse {
            header: self.header(),
            timestamp,
        }))
    }
    pub(crate) async fn set_external_timestamp_request(
        &self,
        request: pdpb::SetExternalTimestampRequest,
    ) -> Result<Response<pdpb::SetExternalTimestampResponse>, Status> {
        self.check(request.header.as_ref()).await?;
        let result = tokio::select! { biased;
            _ = self.cancel.cancelled() => return Err(Status::unavailable("server not started")),
            result = async {
                let now = self.services.timestamp(1).await.map_err(Status::unknown)?;
                let global = (now.physical << 18) | u64::from(now.logical);
                let result = if request.timestamp > global {
                    Err("the external timestamp should not be larger than global ts".into())
                } else if !self.services.is_bootstrapped().await.map_err(Status::unknown)? {
                    Err("[PD:cluster:ErrNotBootstrapped]TiKV cluster not bootstrapped, please start TiKV first".into())
                } else {
                    self.services.set_external_timestamp(request.timestamp).await
                };
                Ok::<_,Status>(result)
            } => result?,
        };
        let mut header = self.header().unwrap();
        if let Err(message) = result {
            header.error = Some(pdpb::Error {
                r#type: pdpb::ErrorType::InvalidValue as i32,
                message,
            });
        }
        Ok(Response::new(pdpb::SetExternalTimestampResponse {
            header: Some(header),
        }))
    }
}
