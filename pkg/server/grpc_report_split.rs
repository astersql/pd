// Copyright 2026 AsterSQL.
//! Split reports validate adjacency; Go leaves region updates to heartbeats.
use crate::grpc::Grpc;
use astersql_pd_proto::{metapb, pdpb};
use tonic::Status;
impl Grpc {
    pub(crate) async fn report_split_header(
        &self,
        header: Option<&pdpb::RequestHeader>,
        regions: &[Option<metapb::Region>],
    ) -> Result<Option<pdpb::ResponseHeader>, Status> {
        self.check(header).await?;
        if let Some(header) = self.bootstrap_header().await? {
            return Ok(Some(header));
        }
        // Match checkSplitRegion/checkSplitRegions, including the unbounded end.
        // Neither epoch, IDs nor current parent membership participates in Go.
        let valid = regions.len() > 1
            && regions.windows(2).all(|pair| match (&pair[0], &pair[1]) {
                (Some(left), Some(right)) => {
                    left.end_key == right.start_key
                        && (right.end_key.is_empty() || left.start_key < right.end_key)
                }
                _ => false,
            });
        let mut header = self.header().unwrap();
        if !valid {
            header.error = Some(pdpb::Error {
                r#type: pdpb::ErrorType::Unknown as i32,
                message: "invalid split region".into(),
            });
        }
        Ok(Some(header))
    }
}
