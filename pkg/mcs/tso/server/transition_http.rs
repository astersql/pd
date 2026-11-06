// Copyright 2026 AsterSQL.
//! The independent host uses the same leader-mediated completion route as Go.
use astersql_pd_mcs_server::ClientTlsFactory;
use astersql_pd_storage_domain::KeyspaceGroup;
use astersql_pd_tso::TsoError;
use tokio_util::sync::CancellationToken;
pub(crate) async fn finish(
    base: &str,
    group: &KeyspaceGroup,
    cancel: &CancellationToken,
    tls: Option<&ClientTlsFactory>,
) -> Result<(), TsoError> {
    let unavailable = || TsoError::Store(astersql_pd_storage_domain::StoreError::Unavailable);
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    let work = async {
        let uri: hyper::Uri = base.parse().map_err(|_| TsoError::InvalidArgument)?;
        let secure = match uri.scheme_str() {
            Some("http") => false,
            Some("https") => true,
            _ => return Err(TsoError::InvalidArgument),
        };
        let host = uri.host().ok_or(TsoError::InvalidArgument)?;
        let addresses = tokio::net::lookup_host((
            host.trim_matches(['[', ']']),
            uri.port_u16().unwrap_or(if secure { 443 } else { 80 }),
        ))
        .await
        .map_err(|_| unavailable())?
        .collect();
        let client_tls = if secure {
            Some(tls.ok_or(TsoError::InvalidArgument)?().map_err(|_| unavailable())?)
        } else {
            None
        };
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
            .body(axum::body::Body::empty())
            .map_err(|_| TsoError::InvalidArgument)?;
        let response = astersql_pd_mcs_server::http_transport::send(
            uri,
            addresses,
            request,
            client_tls,
            cancel.clone(),
            deadline,
        )
        .await
        .map_err(|_| unavailable())?;
        let status = response.status();
        axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .map_err(|_| unavailable())?;
        if status != hyper::StatusCode::OK {
            return Err(TsoError::WindowLost);
        }
        Ok(())
    };
    tokio::select! {biased;_=cancel.cancelled()=>Err(TsoError::Cancelled),r=tokio::time::timeout_at(deadline,work)=>r.map_err(|_|unavailable())?}
}
