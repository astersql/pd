// Copyright 2026 AsterSQL.
//! Keyspace unary operations share the HTTP host manager.
use crate::grpc::Grpc;
use astersql_pd_keyspace::KeyspaceError;
use astersql_pd_proto::{
    keyspacepb::{self as k, keyspace_server::Keyspace},
    pdpb,
};
use astersql_pd_storage_domain::{Keyspace as Meta, KeyspaceIdentity};
use tonic::{Request, Response, Status};
fn wire(m: Meta) -> k::KeyspaceMeta {
    k::KeyspaceMeta {
        keyspace: m.identity.map(|id| match id {
            KeyspaceIdentity::Id(id) => k::keyspace_meta::Keyspace::Id(id),
            KeyspaceIdentity::Scoped {
                namespace_id,
                keyspace_id,
            } => k::keyspace_meta::Keyspace::KeyspaceIdentity(
                astersql_pd_proto::apipb::KeyspaceIdentity {
                    namespace_id,
                    keyspace_id,
                },
            ),
        }),
        name: m.name,
        state: m.state,
        created_at: m.created_at,
        state_changed_at: m.state_changed_at,
        config: m.config,
    }
}
fn result(
    r: Result<Meta, KeyspaceError>,
) -> (Option<pdpb::ResponseHeader>, Option<k::KeyspaceMeta>) {
    match r {
        Ok(m) => (Some(pdpb::ResponseHeader::default()), Some(wire(m))),
        Err(e) => (error(e), None),
    }
}
fn error(e: KeyspaceError) -> Option<pdpb::ResponseHeader> {
    Some(pdpb::ResponseHeader {
        error: Some(pdpb::Error {
            r#type: match e {
                KeyspaceError::NotFound => pdpb::ErrorType::EntryNotFound as i32,
                KeyspaceError::Exists => pdpb::ErrorType::DuplicatedEntry as i32,
                _ => pdpb::ErrorType::Unknown as i32,
            },
            message: e.to_string(),
        }),
        ..Default::default()
    })
}
#[tonic::async_trait]
impl Keyspace for Grpc {
    async fn load_keyspace(
        &self,
        r: Request<k::LoadKeyspaceRequest>,
    ) -> Result<Response<k::LoadKeyspaceResponse>, Status> {
        let r = r.into_inner();
        self.check(r.header.as_ref()).await?;
        let m = self
            .services
            .keyspace_manager()
            .await
            .map_err(Status::unknown)?;
        let loaded = match m.load_by_name(&r.name).await {
            Ok(meta) if !m.check_region_bound(&meta) => Err(KeyspaceError::NotFound),
            r => r,
        };
        let (header, keyspace) = result(loaded);
        Ok(Response::new(k::LoadKeyspaceResponse { header, keyspace }))
    }
    async fn load_keyspace_by_id(
        &self,
        r: Request<k::LoadKeyspaceByIdRequest>,
    ) -> Result<Response<k::LoadKeyspaceResponse>, Status> {
        let r = r.into_inner();
        self.check(r.header.as_ref()).await?;
        let id = match r.keyspace {
            Some(k::load_keyspace_by_id_request::Keyspace::Id(id)) => id,
            _ => 0,
        };
        let m = self
            .services
            .keyspace_manager()
            .await
            .map_err(Status::unknown)?;
        let (header, keyspace) = result(m.load_by_id(id).await);
        Ok(Response::new(k::LoadKeyspaceResponse { header, keyspace }))
    }
    async fn get_all_keyspaces(
        &self,
        r: Request<k::GetAllKeyspacesRequest>,
    ) -> Result<Response<k::GetAllKeyspacesResponse>, Status> {
        let r = r.into_inner();
        self.check(r.header.as_ref()).await?;
        let start = match r.start_keyspace {
            Some(k::get_all_keyspaces_request::StartKeyspace::StartId(id)) => id,
            _ => 0,
        };
        let m = self
            .services
            .keyspace_manager()
            .await
            .map_err(Status::unknown)?;
        let (header, keyspaces) = match m.load_range(start, r.limit as usize).await {
            Ok(v) => (
                Some(pdpb::ResponseHeader::default()),
                v.into_iter().map(wire).collect(),
            ),
            Err(e) => (error(e), vec![]),
        };
        Ok(Response::new(k::GetAllKeyspacesResponse {
            header,
            keyspaces,
        }))
    }
    async fn update_keyspace_state(
        &self,
        r: Request<k::UpdateKeyspaceStateRequest>,
    ) -> Result<Response<k::UpdateKeyspaceStateResponse>, Status> {
        let r = r.into_inner();
        self.check(r.header.as_ref()).await?;
        let id = match r.keyspace {
            Some(k::update_keyspace_state_request::Keyspace::Id(id)) => id,
            _ => 0,
        };
        let m = self
            .services
            .keyspace_manager()
            .await
            .map_err(Status::unknown)?;
        let (header, keyspace) = result(
            m.update_state_by_id(id, r.state, crate::grpc_gc_barrier::now()?.unix_seconds())
                .await,
        );
        Ok(Response::new(k::UpdateKeyspaceStateResponse {
            header,
            keyspace,
        }))
    }
    async fn lookup_keyspace(
        &self,
        _: Request<k::LookupKeyspaceRequest>,
    ) -> Result<Response<k::LookupKeyspaceResponse>, Status> {
        Err(Status::unimplemented(
            "method LookupKeyspace not implemented",
        ))
    }
    async fn watch_keyspaces(
        &self,
        r: Request<k::WatchKeyspacesRequest>,
    ) -> Result<
        Response<
            std::pin::Pin<
                Box<
                    dyn tokio_stream::Stream<Item = Result<k::WatchKeyspacesResponse, Status>>
                        + Send,
                >,
            >,
        >,
        Status,
    > {
        self.check(r.get_ref().header.as_ref()).await?;
        let m = self
            .services
            .keyspace_manager()
            .await
            .map_err(Status::unknown)?;
        let cancel = self.cancel.child_token();
        let (initial, mut watch) = m
            .watch_metadata(&cancel)
            .await
            .map_err(|e| Status::unknown(e.to_string()))?;
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            let _guard = cancel.clone().drop_guard();
            let mut metas = initial;
            loop {
                if !metas.is_empty() {
                    let message = k::WatchKeyspacesResponse {
                        header: Some(pdpb::ResponseHeader::default()),
                        keyspaces: metas.into_iter().map(wire).collect(),
                    };
                    tokio::select! {biased;_=cancel.cancelled()=>break,r=tx.send(Ok(message))=>if r.is_err(){break;}}
                }
                let next = tokio::select! {biased;_=cancel.cancelled()=>break,_=tx.closed()=>break,r=watch.next(&cancel)=>r};
                match next {
                    Ok(v) => metas = v,
                    Err(e) => {
                        let _ = tx.send(Err(Status::unknown(e.to_string()))).await;
                        break;
                    }
                }
            }
        });
        Ok(Response::new(Box::pin(
            tokio_stream::wrappers::ReceiverStream::new(rx),
        )))
    }
}
