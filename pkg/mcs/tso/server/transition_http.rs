// Copyright 2026 AsterSQL.
//! The independent host uses the same leader-mediated completion route as Go.
use astersql_pd_storage_domain::KeyspaceGroup;
use astersql_pd_tso::TsoError;
use http_body_util::BodyExt;
use tokio_util::sync::CancellationToken;
pub(crate) async fn finish(
    base: &str,
    group: &KeyspaceGroup,
    cancel: &CancellationToken,
) -> Result<(), TsoError> {
    let work = async {
        let uri: hyper::Uri = base.parse().map_err(|_| TsoError::InvalidArgument)?;
        if uri.scheme_str() != Some("http") {
            return Err(TsoError::InvalidArgument);
        }
        let host = uri.host().ok_or(TsoError::InvalidArgument)?;
        let socket = tokio::net::TcpStream::connect((
            host.trim_matches(['[', ']']),
            uri.port_u16().unwrap_or(80),
        ))
        .await
        .map_err(|_| TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable))?;
        let (mut sender, connection) =
            hyper::client::conn::http1::handshake(hyper_util::rt::TokioIo::new(socket))
                .await
                .map_err(|_| {
                    TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable)
                })?;
        let operation = if group.is_split_target() {
            "split"
        } else {
            "merge"
        };
        let request = hyper::Request::builder()
            .method("DELETE")
            .uri(format!(
                "/pd/api/v2/tso/keyspace-groups/{}/{operation}",
                group.id
            ))
            .header(
                "host",
                uri.authority().ok_or(TsoError::InvalidArgument)?.as_str(),
            )
            .body(http_body_util::Empty::<hyper::body::Bytes>::new())
            .map_err(|_| TsoError::InvalidArgument)?;
        let response = async {
            let response = sender.send_request(request).await.map_err(|_| {
                TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable)
            })?;
            let status = response.status();
            let _body = response.into_body().collect().await.map_err(|_| {
                TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable)
            })?;
            if status != hyper::StatusCode::OK {
                return Err(TsoError::WindowLost);
            }
            Ok(())
        };
        tokio::pin!(connection);
        tokio::select! {biased;r=response=>r,_=&mut connection=>Err(TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable))}
    };
    tokio::select! {biased;_=cancel.cancelled()=>Err(TsoError::Cancelled),r=tokio::time::timeout(std::time::Duration::from_secs(2),work)=>r.map_err(|_|TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable))?}
}
